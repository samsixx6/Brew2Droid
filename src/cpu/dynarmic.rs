//! Backend ARM sobre Dynarmic.
//!
//! Dynarmic recompila os blocos A32 para o código nativo do host. Este arquivo mantém a mesma
//! fronteira do [`CpuBackend`]: os endereços não mapeados das vtables BREW continuam sendo a
//! parada que devolve o controle ao despachante Rust.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::mem::size_of;
use std::rc::Rc;

use dynarmic::a32::{ArchVersion, Callbacks, Dynarmic as Jit, VAddr};
use dynarmic::{CallbackImpl, GuestInt, HaltReason};

use super::mem::GuestMemory;
use super::{API_BASE, API_SIZE, RETURN_MAGIC};
use super::{CpuBackend, CpuError, Reg, StopReason};

/// `CPSR` de modo usuário do ARM. A extensão da Superscape confere este campo antes de tocar
/// nas tabelas da MMU; o applet BREW nunca é código privilegiado.
const MODO_USUARIO: u32 = 0x10;

/// O bit `T` do `CPSR`: ligado, o núcleo busca instruções Thumb.
const CPSR_THUMB: u32 = 1 << 5;

/// Limite defensivo da string recebida por `SYS_WRITE0`: uma string sem terminador não pode
/// prender o host em uma leitura sem fim.
const MAX_SEMIHOSTING_STRING: u32 = 4096;

/// Granularidade que o Dynarmic usa para indexar código recompilado.
const PAGE: u32 = 4096;

/// Quantas páginas de 4 KB cabem nos 32 bits do guest: o tamanho da tabela de páginas.
const PAGINAS: usize = 1 << 20;

/// A tabela compartilha o ponteiro de leitura com um atributo: bit zero exige callback de escrita.
/// Só ponteiros host alinhados a oito bytes entram nela; o JIT retira o atributo antes do acesso.
const ESCRITA_INTERCEPTADA: usize = 1;

/// Bytes reservados além do fim de cada região gravável. Uma leitura de quatro bytes nos últimos
/// bytes de uma página que termina a região cruza a borda na memória do host; com a folga, ela
/// lê memória alocada em vez de sair do vetor.
const FOLGA_DA_REGIAO: usize = 16;

/// O que uma callback pediu que `run` devolva. O Dynarmic recebe os acessos de memória dentro
/// do bloco recompilado; guardar o motivo aqui preserva a distinção entre API, retorno e falha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parada {
    Nenhuma,
    Fetch(u32),
    Memoria(u32),
    Excecao(u32),
}

/// As páginas do guest que já foram buscadas como código, um bit por página.
///
/// Toda escrita do guest pergunta por aqui, e o renderizador ARM do Kingdom Hearts escreve
/// milhões de pixels por quadro. Com um `BTreeSet` atrás de um `RefCell`, cada pixel pagava um
/// empréstimo e uma busca em árvore — e era isso que fazia o 3D, já acelerado pelo JIT, voltar a
/// perder quadros. Um bit por página, em `Cell`, responde com um deslocamento e uma máscara: são
/// 2^20 páginas de 4 KB, 16.384 palavras, 131 KB.
struct PaginasExecutadas {
    bits: Box<[Cell<u64>]>,
}

impl PaginasExecutadas {
    fn new() -> Self {
        Self {
            bits: (0..(1usize << 20).div_ceil(64))
                .map(|_| Cell::new(0))
                .collect(),
        }
    }

    fn marca(&self, pagina: u32) {
        let (palavra, bit) = ((pagina / 64) as usize, pagina % 64);
        if let Some(celula) = self.bits.get(palavra) {
            celula.set(celula.get() | (1 << bit));
        }
    }

    fn contem(&self, pagina: u32) -> bool {
        let (palavra, bit) = ((pagina / 64) as usize, pagina % 64);
        self.bits
            .get(palavra)
            .is_some_and(|celula| celula.get() & (1 << bit) != 0)
    }
}

/// Estado compartilhado entre as callbacks C++ e o invólucro Rust.
///
/// O JIT é guardado em `Box` no backend, então seu endereço não muda quando o `DynarmicCpu`
/// se move. Isso permite à callback interromper a execução no instante em que o PC entra numa
/// vtable BREW, em vez de executar uma instrução falsa naquele endereço.
struct Estado {
    memoria: Rc<RefCell<GuestMemory>>,
    semihosting: Rc<RefCell<String>>,
    /// Só uma escrita em memória que já foi buscada como código pode invalidar um bloco JIT.
    /// Isto exclui os milhões de escritas nos buffers RGB565.
    paginas_executadas: PaginasExecutadas,
    /// Invalidações pedidas pelo ARM durante o bloco em execução; são aplicadas após `run`.
    codigo_sujo: RefCell<BTreeSet<u32>>,
    /// As faixas de [`CpuBackend::watch_dirty`]: `(id, início, fim, sujo)`.
    vigias: RefCell<Vec<(u32, u32, u32, bool)>>,
    /// O menor intervalo que contém todas as vigias. Quase toda escrita do guest cai fora dele,
    /// e aí ela custa duas comparações em vez de uma volta pela lista.
    envoltorio: Cell<(u32, u32)>,
    /// Uma faixa que já está suja. Escrita dentro dela não precisa de mais nada.
    ///
    /// É o caso quase sempre: um renderizador ARM escreve milhões de pixels seguidos no mesmo
    /// buffer, e depois do primeiro todos caem numa faixa que já está marcada. Sem o atalho,
    /// cada um desses pegava a lista emprestada e a percorria para marcar o que já estava
    /// marcado. Zera sempre que alguma faixa pode ter voltado a limpa.
    atalho: Cell<(u32, u32)>,
    instrucoes: Cell<u64>,
    limite: Cell<u64>,
    parada: Cell<Parada>,
    /// Quantas vezes o host entrou no JIT, e quanto tempo ficou lá dentro.
    ///
    /// **É o número que separa o guest do despacho.** Cada chamada de API é uma saída e uma
    /// reentrada, então o que sobra do relógio depois de descontar o tempo passado dentro do
    /// `jit.run` é, quase todo, trampolim mais corpo do método. Sem esta conta o custo por
    /// chamada de API é desconhecido — e foi tratando um número da era do Unicorn como atual que
    /// a revisão externa errou. Ver [`CpuBackend::relato_do_jit`].
    ///
    /// Ficam aqui, e não no `DynarmicCpu`, porque o empréstimo do `Jit` está vivo durante toda a
    /// `run`: um `Cell` no estado atravessa o empréstimo imutável sem brigar com ele.
    ///
    /// **Contar é de graça; cronometrar não é.** Medido: um par de `Instant::now()` por entrada
    /// custava **40% do relógio** nesta máquina (o `Instant::now` daqui é chamada de sistema, não
    /// o caminho rápido do vDSO), e são 1,3 milhão de entradas num Quake de quinze segundos. Por
    /// isso a contagem é sempre ligada — uma soma num `Cell` — e o relógio é **amostrado** e só
    /// quando alguém pede o perfil de custo. Ver [`liga_medicao_do_jit`].
    entradas_no_jit: Cell<u64>,
    nanos_no_jit: Cell<u64>,
    amostras_no_jit: Cell<u64>,
    jit: Cell<*mut Jit<Estado>>,
    /// Entradas de leitura com bit de proteção de escrita. Nunca desreferenciar uma entrada
    /// marcada no Rust: só o emissor JIT retira o atributo. Ver [`DynarmicCpu::tabela`].
    tabela: *mut *mut u8,
    contagem_de_memoria: Cell<Option<super::MemoryCallbacks>>,
}

impl Estado {
    fn conta_acesso(&self, addr: u32, len: usize, escrita: bool) {
        let Some(mut counts) = self.contagem_de_memoria.get() else { return; };
        let fim = u64::from(addr) + len as u64;
        let vigiada = self.vigias.borrow().iter()
            .any(|v| u64::from(addr) < u64::from(v.2) && fim > u64::from(v.1));
        let em_codigo = (u64::from(addr) / u64::from(PAGE)..=(fim - 1) / u64::from(PAGE))
            .any(|page| self.paginas_executadas.contem(page as u32));
        if escrita {
            counts.escritas += 1;
            counts.escritas_vigiadas += u64::from(vigiada);
            counts.escritas_em_codigo += u64::from(em_codigo);
        } else {
            counts.leituras += 1;
            counts.leituras_vigiadas += u64::from(vigiada);
            counts.leituras_em_codigo += u64::from(em_codigo);
        }
        self.contagem_de_memoria.set(Some(counts));
    }

    /// Intercepta escritas conservando leituras diretas quando a página já tem ponteiro seguro.
    fn anula_pagina(&self, pagina: u32) {
        if (pagina as usize) < PAGINAS {
            let entry = unsafe { &mut *self.tabela.add(pagina as usize) };
            if !entry.is_null() {
                *entry = entry.map_addr(|addr| addr | ESCRITA_INTERCEPTADA);
            }
        }
    }

    /// Uma faixa já suja não precisa interceptar pixels até take_dirty. A página só é
    /// liberada para escrita quando TODAS as vigias dela estão sujas. Leituras não sujam nada.
    fn atualiza_paginas(&self, inicio: u32, fim: u32) {
        if inicio >= fim { return; }
        let vigias = self.vigias.borrow();
        let memoria = self.memoria.borrow();
        for pagina in (inicio / PAGE)..=((fim - 1) / PAGE) {
            let de = u64::from(pagina) * u64::from(PAGE);
            let ate = de + u64::from(PAGE);
            let interceptar = self.paginas_executadas.contem(pagina)
                || vigias.iter().any(|v| !v.3 && de < u64::from(v.2) && ate > u64::from(v.1));
            let pointer = ponteiro_da_pagina(&memoria, pagina, interceptar);
            unsafe { *self.tabela.add(pagina as usize) = pointer; }
        }
    }

    fn para(cb: &CallbackImpl<Self>, parada: Parada) {
        if cb.parada.get() == Parada::Nenhuma {
            cb.parada.set(parada);
        }
        let jit = cb.jit.get();
        if !jit.is_null() {
            // `HaltExecution` é a via documentada pelo Dynarmic para uma callback devolver o
            // controle ao host. A razão concreta é mantida em `parada`, pois o enum público só
            // oferece os bits genéricos de usuário.
            // MemoryAbort é o bit que os emissores x64/AArch64 testam após o acesso.
            // Um bit de usuário só interrompe no fim do bloco, depois de escritas indevidas.
            let motivo = if matches!(parada, Parada::Memoria(_)) {
                HaltReason::MemoryAbort
            } else {
                HaltReason::UserDefined1
            };
            unsafe { (*jit).halt(motivo) };
        }
    }

    fn le(&self, addr: u32, out: &mut [u8]) -> bool {
        self.memoria
            .borrow()
            .read(addr, out.len() as u32)
            .map_or_else(
                |_| false,
                |bytes| {
                    out.copy_from_slice(bytes);
                    true
                },
            )
    }

    fn escreve(&self, addr: u32, bytes: &[u8]) -> bool {
        if self.memoria.borrow_mut().write(addr, bytes).is_err() {
            return false;
        }
        self.marca_codigo_sujo(addr, bytes.len() as u32);
        self.marca_vigias(addr, addr.saturating_add(bytes.len() as u32));
        true
    }

    fn marca_vigias(&self, inicio: u32, fim: u32) {
        let (menor, maior) = self.envoltorio.get();
        if fim <= menor || inicio >= maior {
            return;
        }
        let (a, b) = self.atalho.get();
        if inicio >= a && fim <= b {
            return;
        }
        let mut vigias = self.vigias.borrow_mut();
        let mut tocadas = 0;
        let mut faixa = (0, 0);
        let mut novas = Vec::new();
        for vigia in vigias.iter_mut() {
            if inicio < vigia.2 && fim > vigia.1 {
                if !vigia.3 { novas.push((vigia.1, vigia.2)); }
                vigia.3 = true;
                tocadas += 1;
                faixa = (vigia.1, vigia.2);
            }
        }
        // Uma escrita pode tocar só a parte exclusiva de uma faixa sobreposta. Não guardar
        // a faixa inteira enquanto outra vigia limpa a intercepta: o próximo pixel pode cair nela.
        let sobrepoe_limpa = vigias.iter().any(|v| !v.3 && faixa.0 < v.2 && faixa.1 > v.1);
        if tocadas == 1 && inicio >= faixa.0 && fim <= faixa.1 && !sobrepoe_limpa {
            self.atalho.set(faixa);
        }
        drop(vigias);
        for (de, ate) in novas { self.atualiza_paginas(de, ate); }
    }

    fn recalcula_envoltorio(&self) {
        self.atalho.set((0, 0));
        let vigias = self.vigias.borrow();
        let menor = vigias.iter().map(|v| v.1).min().unwrap_or(0);
        let maior = vigias.iter().map(|v| v.2).max().unwrap_or(0);
        self.envoltorio.set((menor, maior));
    }

    fn marca_codigo_sujo(&self, addr: u32, len: u32) {
        let fim = addr.saturating_add(len.saturating_sub(1));
        for pagina in (addr / PAGE)..=(fim / PAGE) {
            // O teste de um bit vem antes de qualquer empréstimo: quase toda escrita cai em
            // página de dados, e só a que cai em código paga a lista de sujas.
            if self.paginas_executadas.contem(pagina) {
                self.codigo_sujo.borrow_mut().insert(pagina);
            }
        }
    }
}

impl Callbacks for Estado {
    fn memory_read_code(cb: &CallbackImpl<Self>, addr: VAddr) -> Option<u32> {
        let mut bytes = [0; 4];
        if cb.memoria.borrow().executavel(addr) && cb.le(addr, &mut bytes) {
            // Código continua legível, mas toda escrita deve chegar à invalidação JIT.
            let pagina = addr / PAGE;
            if !cb.paginas_executadas.contem(pagina) {
                cb.paginas_executadas.marca(pagina);
                cb.anula_pagina(pagina);
            }
            Some(u32::from_le_bytes(bytes))
        } else {
            Self::para(cb, Parada::Fetch(addr));
            None
        }
    }

    extern "C" fn memory_read<T: GuestInt>(cb: &CallbackImpl<Self>, addr: VAddr) -> T {
        let mut bytes = [0u8; 8];
        let len = size_of::<T>();
        cb.conta_acesso(addr, len, false);
        if !cb.le(addr, &mut bytes[..len]) {
            Self::para(cb, Parada::Memoria(addr));
        }
        // Os hosts aceitos pelo Dynarmic são x86-64 e AArch64, ambos little-endian. `T` só é
        // instanciado pelo JIT para u8/u16/u32/u64.
        unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) }
    }

    extern "C" fn memory_write<T: GuestInt>(cb: &mut CallbackImpl<Self>, addr: VAddr, value: T) {
        let len = size_of::<T>();
        cb.conta_acesso(addr, len, true);
        let bytes = unsafe { std::slice::from_raw_parts((&value as *const T).cast::<u8>(), len) };
        if !cb.escreve(addr, bytes) {
            Self::para(cb, Parada::Memoria(addr));
        }
    }

    extern "C" fn call_svc(cb: &mut CallbackImpl<Self>, swi: u32) {
        // `SVC #0xAB` é o semihosting ARM que Peggle e Zuma usam para log. Não é uma
        // interrupção BREW: depois de atendê-la a execução continua na instrução seguinte.
        // Reconhecemos as duas operações de saída que os jogos observados usam e devolvemos zero
        // nas outras, como o monitor ARM faz para a maioria das consultas inofensivas.
        //
        // **O número muda com o estado do núcleo**: `0xAB` é o do Thumb, e `0x123456` o do ARM.
        // O Ultimate Chess 3D escreve o log dele em ARM, e com só o `0xAB` reconhecido o
        // primeiro `SYS_WRITEC` — ao comer uma peça do adversário — parava o jogo com "exceção
        // do núcleo ARM". O interpretador já atendia os dois: ele olha só o `r0`.
        if SEMIHOSTING.contains(&swi) {
            let jit = unsafe { &mut *cb.jit.get() };
            let op = jit.get_reg(0);
            let arg = jit.get_reg(1);
            let mut saida = cb.semihosting.borrow_mut();
            match op {
                0x03 => {
                    let mut byte = [0u8; 1];
                    if cb.le(arg, &mut byte) {
                        saida.push(byte[0] as char);
                    }
                }
                0x04 => {
                    for offset in 0..MAX_SEMIHOSTING_STRING {
                        let mut byte = [0u8; 1];
                        if !cb.le(arg.saturating_add(offset), &mut byte) || byte[0] == 0 {
                            break;
                        }
                        saida.push(byte[0] as char);
                    }
                }
                _ => {}
            }
            super::apara_semihosting(&mut saida);
            jit.set_reg(0, 0);
        } else if cb.parada.get() == Parada::Nenhuma {
            let pc = unsafe { (*cb.jit.get()).get_pc() };
            Self::para(cb, Parada::Excecao(pc));
        }
    }

    extern "C" fn add_ticks(cb: &mut CallbackImpl<Self>, ticks: u64) {
        cb.instrucoes.set(cb.instrucoes.get().saturating_add(ticks));
    }

    extern "C" fn get_ticks_remaining(cb: &CallbackImpl<Self>) -> u64 {
        cb.limite.get().saturating_sub(cb.instrucoes.get())
    }

    extern "C" fn raised_exception(
        cb: &mut CallbackImpl<Self>,
        pc: VAddr,
        _exception: ::dynarmic::a32::Exception,
    ) {
        // `MemoryReadCode(None)` termina como `NoExecuteFault`, que chega aqui depois de a
        // callback já ter registrado o fetch. Não sobrescrevê-lo é o que mantém a fronteira
        // das vtables BREW distinguível de uma instrução realmente inválida.
        if cb.parada.get() == Parada::Nenhuma {
            // A binding atual transforma `optional<u32>::none()` do fetch em
            // `NoExecuteFault` antes de devolver o controle. A faixa das APIs e o sentinela
            // nunca contêm código por definição, então ainda dá para classificá-los aqui sem
            // ambiguidade.
            if pc == RETURN_MAGIC || (API_BASE..API_BASE.saturating_add(API_SIZE)).contains(&pc) {
                Self::para(cb, Parada::Fetch(pc));
            } else {
                Self::para(cb, Parada::Excecao(pc));
            }
        }
    }
}

/// Os números de `SVC` do semihosting ARM: o do Thumb e o do ARM.
const SEMIHOSTING: [u32; 2] = [0xab, 0x12_3456];

/// De quantas em quantas entradas no JIT o relógio é lido, quando a medição está ligada.
const AMOSTRA_DO_JIT: u64 = 64;

/// Recompilador A32. Fica separado do backend padrão até a equivalência ser estabelecida jogo
/// a jogo; criar a CPU não aloca o JIT, porque o mapa do guest só existe em `reset`.
pub struct DynarmicCpu {
    memoria: Rc<RefCell<GuestMemory>>,
    semihosting: Rc<RefCell<String>>,
    jit: Option<Box<Jit<Estado>>>,
    /// **A tabela de páginas: o acesso à memória sem callback.** Sem ela, cada leitura e escrita
    /// do código recompilado saía do JIT para uma callback Rust, pegava o mapa por `RefCell` e
    /// procurava a região. No Need for Speed, na corrida, isso deixava o emulador em 176 milhões
    /// de instruções por segundo — 69% da velocidade do console.
    ///
    /// Cada entrada guarda o início estável de uma página completa, com bit zero marcado para
    /// código executado, regiões somente leitura e vigias limpas. Nulo exige callback nos dois
    /// sentidos: páginas parciais, não mapeadas ou cujo ponteiro não tem alinhamento de oito bytes.
    tabela: Box<[*mut u8]>,
}

impl DynarmicCpu {
    pub fn new() -> Result<Self, CpuError> {
        Ok(Self {
            memoria: Default::default(),
            semihosting: Default::default(),
            jit: None,
            tabela: vec![std::ptr::null_mut(); PAGINAS].into_boxed_slice(),
        })
    }

    fn jit(&self) -> Result<&Jit<Estado>, CpuError> {
        self.jit
            .as_deref()
            .ok_or_else(|| CpuError("Dynarmic não recebeu mapa de memória".into()))
    }

    fn jit_mut(&mut self) -> Result<&mut Jit<Estado>, CpuError> {
        self.jit
            .as_deref_mut()
            .ok_or_else(|| CpuError("Dynarmic não recebeu mapa de memória".into()))
    }

    fn indice(reg: Reg) -> usize {
        match reg {
            Reg::R0 => 0,
            Reg::R1 => 1,
            Reg::R2 => 2,
            Reg::R3 => 3,
            Reg::R4 => 4,
            Reg::R5 => 5,
            Reg::R6 => 6,
            Reg::R7 => 7,
            Reg::R8 => 8,
            Reg::R9 => 9,
            Reg::R10 => 10,
            Reg::R11 => 11,
            Reg::R12 => 12,
            Reg::Sp => 13,
            Reg::Lr => 14,
            Reg::Pc => 15,
        }
    }

    /// A interface de diagnóstico chama este método nos dois núcleos.
    pub fn semihosting(&self) -> String {
        self.semihosting.borrow().clone()
    }

    /// A API host também escreve memória do guest, sempre entre duas entradas no JIT. Caso ela
    /// altere uma página que já foi executada, o próximo bloco deve ser recompilado.
    fn invalida_codigo_escrito(&mut self, addr: u32, len: u32) {
        let Ok(jit) = self.jit_mut() else {
            return;
        };
        jit.marca_codigo_sujo(addr, len);
        let paginas = std::mem::take(&mut *jit.codigo_sujo.borrow_mut());
        // **Quantas páginas de código caíram por escrita do host.** É o número que responde se
        // leitura/escrita em página já executada é evento raro (nada a fazer) ou caminho quente
        // (candidato a leitura direta com armadilha de escrita). Sem ele, o custo do SMC é
        // invisível no perfil: aparece diluído no despacho, como "alguma chamada de API".
        if !paginas.is_empty() {
            crate::registro!(
                crate::registro::Nivel::Depuracao,
                "cpu",
                "escrita em {addr:#010x}+{len} invalidou {} página(s) de código",
                paginas.len()
            );
        }
        for pagina in paginas {
            jit.invalidate_cache_range(pagina * PAGE, PAGE as usize);
        }
    }
}

impl CpuBackend for DynarmicCpu {
    fn reset(&mut self, mem: &GuestMemory) -> Result<(), CpuError> {
        // `GuestMemory` é deliberadamente construído uma vez antes da execução. Copiar o mapa
        // para a memória que as callbacks possuem mantém o contrato do backend: a API Rust e o
        // ARM enxergam os mesmos bytes a partir daqui.
        let mut copia = GuestMemory::new();
        for regiao in mem.regions() {
            copia
                .map_com_execucao(
                    regiao.name,
                    regiao.base,
                    regiao.bytes.clone(),
                    regiao.writable,
                    regiao.executavel,
                )
                .map_err(|e| CpuError(e.to_string()))?;
        }
        for regiao in copia.regions_mut() {
            regiao.bytes.reserve_exact(FOLGA_DA_REGIAO);
        }
        self.tabela.fill(std::ptr::null_mut());
        for pagina in 0..PAGINAS as u32 {
            self.tabela[pagina as usize] = ponteiro_da_pagina(&copia, pagina, false);
        }
        // Contar legíveis separadamente de escrita: código e vigias continuam tendo proteção.
        let diretas = self
            .tabela
            .iter()
            .filter(|ponteiro| !ponteiro.is_null())
            .count();
        crate::registro!(
            crate::registro::Nivel::Depuracao,
            "cpu",
            "tabela de páginas: {diretas} de {} com leitura direta ({} region(oes))",
            PAGINAS,
            copia.regions().len()
        );
        self.memoria = Rc::new(RefCell::new(copia));
        self.semihosting.borrow_mut().clear();
        let estado = Estado {
            memoria: self.memoria.clone(),
            semihosting: self.semihosting.clone(),
            paginas_executadas: PaginasExecutadas::new(),
            codigo_sujo: Default::default(),
            vigias: Default::default(),
            envoltorio: Cell::new((0, 0)),
            atalho: Cell::new((0, 0)),
            instrucoes: Cell::new(0),
            limite: Cell::new(0),
            parada: Cell::new(Parada::Nenhuma),
            entradas_no_jit: Cell::new(0),
            nanos_no_jit: Cell::new(0),
            amostras_no_jit: Cell::new(0),
            jit: Cell::new(std::ptr::null_mut()),
            tabela: self.tabela.as_mut_ptr(),
            contagem_de_memoria: Cell::new(None),
        };
        let mut config = Jit::<Estado>::new_config();
        config.arch_ver(ArchVersion::V6K);
        config.check_halt_on_memory_access(true);
        // 64 MiB cabe no desktop. No Switch esse bloco disputa o heap com o jogo
        // e com a Mesa; 32 MiB ainda cobre o código traduzido do Double Dragon
        // quando o título é aberto segurando R.
        config.code_cache_size(if cfg!(zeebx_switch) {
            32 * 1024 * 1024
        } else {
            64 * 1024 * 1024
        });
        // Bit zero conserva leituras rápidas sem contornar as callbacks de escrita.
        config.page_table_mask(1);
        config.page_table_write_protection(true);
        unsafe { config.page_table(self.tabela.as_mut_ptr().cast()) };
        let mut jit = Box::new(config.init(estado));
        let ptr = &mut *jit as *mut Jit<Estado>;
        jit.jit.set(ptr);
        jit.set_cpsr(MODO_USUARIO);
        self.jit = Some(jit);
        Ok(())
    }

    fn read_reg(&self, reg: Reg) -> u32 {
        self.jit().map_or(0, |jit| jit.get_reg(Self::indice(reg)))
    }

    fn write_reg(&mut self, reg: Reg, value: u32) {
        if let Ok(jit) = self.jit_mut() {
            jit.set_reg(Self::indice(reg), value);
        }
    }

    fn instructions(&self) -> u64 {
        self.jit().map_or(0, |jit| jit.instrucoes.get())
    }

    fn relato_do_jit(&self) -> Option<(u64, u64, u64)> {
        self.jit().ok().map(|jit| {
            let entradas = jit.entradas_no_jit.get();
            let amostras = jit.amostras_no_jit.get();
            // A média amostrada vale para todas as entradas: nenhuma delas é especial.
            let nanos = match amostras {
                0 => 0,
                n => jit.nanos_no_jit.get() / n * entradas,
            };
            (entradas, nanos, amostras)
        })
    }

    fn liga_contagem_de_memoria(&mut self, ativa: bool) {
        if let Ok(jit) = self.jit_mut() {
            jit.contagem_de_memoria.set(ativa.then(super::MemoryCallbacks::default));
        }
    }

    fn relato_de_memoria(&self) -> Option<super::MemoryCallbacks> {
        self.jit().ok().and_then(|jit| jit.contagem_de_memoria.get())
    }

    fn set_instructions(&mut self, valor: u64) {
        if let Ok(jit) = self.jit_mut() {
            jit.instrucoes.set(valor);
        }
    }

    fn cpsr(&self) -> u32 {
        self.jit().map_or(0, |jit| jit.get_cpsr())
    }

    fn set_cpsr(&mut self, valor: u32) {
        if let Ok(jit) = self.jit_mut() {
            jit.set_cpsr(valor);
        }
    }

    fn em_thumb(&self) -> bool {
        self.jit().is_ok_and(|jit| jit.get_cpsr() & CPSR_THUMB != 0)
    }

    /// A vigia de escrita: só escrita **do guest** liga o sinalizador.
    ///
    /// Sem ela o contrato padrão responde "sempre sujo", e cada chamada que desenha importava
    /// todas as superfícies inteiras. No Pac-Mania, 100 mil `IIMAGE_Draw` somavam 22 segundos
    /// só de leitura de buffers que o jogo não tinha tocado.
    fn watch_dirty(&mut self, id: u32, base: u32, len: u32) -> Result<(), CpuError> {
        self.unwatch_dirty(id);
        let jit = self.jit_mut()?;
        // Começa sujo: desta faixa ainda não vimos nada.
        jit.vigias
            .borrow_mut()
            .push((id, base, base.saturating_add(len), true));
        jit.recalcula_envoltorio();
        // Nasce suja: o primeiro take_dirty é que arma a interceptação da próxima escrita.
        let fim = base.saturating_add(len);
        jit.atualiza_paginas(base, fim);
        Ok(())
    }

    fn unwatch_dirty(&mut self, id: u32) {
        let Ok(jit) = self.jit_mut() else {
            return;
        };
        let faixa = jit
            .vigias
            .borrow()
            .iter()
            .find(|vigia| vigia.0 == id)
            .map(|vigia| (vigia.1, vigia.2));
        jit.vigias.borrow_mut().retain(|vigia| vigia.0 != id);
        jit.recalcula_envoltorio();
        if let Some((inicio, fim)) = faixa {
            jit.atualiza_paginas(inicio, fim);
        }
    }

    fn take_dirty(&mut self, id: u32) -> bool {
        let Ok(jit) = self.jit_mut() else {
            return true;
        };
        // Uma faixa pode voltar a limpa aqui, então o atalho deixa de valer.
        jit.atalho.set((0, 0));
        let faixa = {
            let mut vigias = jit.vigias.borrow_mut();
            vigias.iter_mut().find(|vigia| vigia.0 == id)
                .map(|v| (v.1, v.2, std::mem::replace(&mut v.3, false)))
        };
        let Some((inicio, fim, suja)) = faixa else { return true; };
        if suja && inicio < fim {
            for pagina in (inicio / PAGE)..=((fim - 1) / PAGE) { jit.anula_pagina(pagina); }
        }
        suja
    }

    fn marca_sujo(&mut self, addr: u32, len: u32) {
        let Ok(jit) = self.jit_mut() else {
            return;
        };
        jit.marca_vigias(addr, addr.saturating_add(len));
    }

    fn read_mem(&self, addr: u32, buf: &mut [u8]) -> Result<(), CpuError> {
        self.memoria
            .borrow()
            .read(addr, buf.len() as u32)
            .map(|bytes| buf.copy_from_slice(bytes))
            .map_err(|e| CpuError(e.to_string()))
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> Result<(), CpuError> {
        self.memoria
            .borrow_mut()
            .write(addr, data)
            .map_err(|e| CpuError(e.to_string()))?;
        self.invalida_codigo_escrito(addr, data.len() as u32);
        Ok(())
    }

    fn fill_mem(&mut self, addr: u32, valor: u8, len: u32) -> Result<(), CpuError> {
        self.memoria
            .borrow_mut()
            .fill(addr, valor, len)
            .map_err(|e| CpuError(e.to_string()))?;
        self.invalida_codigo_escrito(addr, len);
        Ok(())
    }

    fn suja_pilha(&mut self, addr: u32, len: u32) -> Result<(), CpuError> {
        // Sem a invalidação de propósito: pilha nunca virou código (ver o traço). A marcação
        // paga `jit_mut` e dois empréstimos por chamada de API para sair sempre vazia, e era
        // isso que levava os 30% — medido no `bench` do Crash Nitro Kart, que volta de 296%
        // para 368% só sem ela.
        debug_assert!(
            self.jit_mut().is_ok_and(|jit| {
                let fim = addr.saturating_add(len.saturating_sub(1));
                ((addr / PAGE)..=(fim / PAGE)).all(|p| !jit.paginas_executadas.contem(p))
            }),
            "sujeira de pilha em {addr:#010x}+{len} caiu em página executada"
        );
        self.memoria
            .borrow_mut()
            .fill(addr, 0, len)
            .map_err(|e| CpuError(e.to_string()))
    }

    fn run(&mut self, pc: u32, max_instructions: u64) -> Result<StopReason, CpuError> {
        let jit = self.jit_mut()?;
        // **O bit 0 do endereço é o modo, não parte do endereço.** O despachante retoma no `lr`
        // do jeito que ele veio, e o `lr` de uma chamada feita de código Thumb traz o bit 0
        // ligado — é a convenção de interworking do ARM. Aqui ela precisa ser explícita:
        // escrevendo o endereço cru, o Zenonia, que é todo Thumb, voltava de cada API um byte
        // adiante e o núcleo parava numa "instrução" montada com metade de duas.
        let cpsr = jit.get_cpsr();
        match pc & 1 {
            1 => jit.set_cpsr(cpsr | CPSR_THUMB),
            _ => jit.set_cpsr(cpsr & !CPSR_THUMB),
        }
        jit.set_pc(pc & !1);
        // `HaltExecution` deixa a razão armada para a volta que acabou de sair. O próximo
        // trecho começa depois de o despachante BREW ter escrito r0/pc, portanto precisa limpar
        // o bit genérico antes de entrar novamente no JIT.
        jit.clear_halt(HaltReason::UserDefined1);
        jit.parada.set(Parada::Nenhuma);
        jit.limite
            .set(jit.instrucoes.get().saturating_add(max_instructions));
        // O relógio em volta do `jit.run`, e só dele: tudo o que se passa aqui dentro é execução
        // do guest (mais as callbacks de memória, que o próprio JIT chama). O que fica de fora é
        // o despacho — e é aí que o trampolim vive.
        //
        // **Amostrado de propósito, e ligado sempre.** O `Instant::now` desta máquina é chamada de
        // sistema, e um par por entrada custou 40% do relógio. Uma entrada em cada `AMOSTRA` mede
        // o mesmo por 1/64 do preço — cerca de 0,6% do relógio, medido —, e por isso a partilha
        // sai em todo relatório em vez de depender de alguém lembrar de ligá-la. A média
        // amostrada é multiplicada pelo contador depois.
        let entrada = jit.entradas_no_jit.get();
        let cronometrar = entrada % AMOSTRA_DO_JIT == 0;
        let comeco = cronometrar.then(std::time::Instant::now);
        let _ = unsafe { jit.run() };
        if let Some(comeco) = comeco {
            jit.nanos_no_jit.set(
                jit.nanos_no_jit
                    .get()
                    .saturating_add(comeco.elapsed().as_nanos() as u64),
            );
            jit.amostras_no_jit
                .set(jit.amostras_no_jit.get().saturating_add(1));
        }
        jit.entradas_no_jit.set(entrada.saturating_add(1));
        // Não há invalidação para páginas de dados: só código previamente executado chega aqui.
        // É seguro mexer no cache depois de o JIT devolver o controle, nunca da callback.
        let paginas = std::mem::take(&mut *jit.codigo_sujo.borrow_mut());
        for pagina in paginas {
            jit.invalidate_cache_range(pagina * PAGE, PAGE as usize);
        }
        match jit.parada.get() {
            Parada::Nenhuma => Ok(StopReason::Budget),
            Parada::Fetch(addr) if addr == RETURN_MAGIC => Ok(StopReason::Returned),
            Parada::Fetch(addr)
                if (API_BASE..API_BASE.saturating_add(API_SIZE)).contains(&addr) =>
            {
                Ok(StopReason::ApiCall { addr })
            }
            Parada::Fetch(addr) => Ok(StopReason::MemoryFault { addr, pc: addr }),
            Parada::Memoria(addr) => Ok(StopReason::MemoryFault {
                addr,
                pc: jit.get_pc(),
            }),
            Parada::Excecao(pc) => Ok(StopReason::Exception { pc }),
        }
    }
}

/// Página completa e estável, marcada quando escrita exige callback. Ponteiros desalinhados
/// conservam o fallback: alinhamento guest não implica o do host em uma região de base arbitrária.
fn ponteiro_da_pagina(memoria: &GuestMemory, pagina: u32, interceptar: bool) -> *mut u8 {
    let inicio = u64::from(pagina) * u64::from(PAGE);
    let fim = inicio + u64::from(PAGE);
    memoria
        .regions()
        .iter()
        .find(|r| {
            inicio >= u64::from(r.base)
                && fim <= u64::from(r.base) + r.bytes.len() as u64
        })
        .map_or(std::ptr::null_mut(), |r| {
            // A região não é realocada depois do `reset`: o vetor só é escrito, nunca cresce.
            let ptr = unsafe { r.bytes.as_ptr().add((inicio - u64::from(r.base)) as usize) as *mut u8 };
            if ptr.addr() & 7 != 0 {
                std::ptr::null_mut()
            } else if interceptar || !r.writable {
                ptr.map_addr(|addr| addr | ESCRITA_INTERCEPTADA)
            } else {
                ptr
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escrita_direta(cpu: &DynarmicCpu, pagina: usize) -> bool {
        let ptr = cpu.tabela[pagina];
        !ptr.is_null() && ptr.addr() & ESCRITA_INTERCEPTADA == 0
    }

    fn cpu_with(code: &[u8]) -> DynarmicCpu {
        let mut mem = GuestMemory::new();
        let mut bytes = code.to_vec();
        bytes.resize(0x1000, 0);
        // A imagem dos módulos do Zeebo é gravável: extensões podem montar pequenas rotinas ou
        // tabelas em RAM. Deixar o código do teste gravável permite conferir a invalidação JIT.
        mem.map("code", 0, bytes, true).unwrap();
        mem.map_zeroed("data", 0x1000, 0x1000).unwrap();
        mem.map_zeroed("stack", 0x2000_0000, 0x1000).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.write_reg(Reg::Sp, 0x2000_0f00);
        cpu
    }

    #[test]
    fn executa_instrucao_e_le_registrador() {
        let code = [0xe3a0_0037u32.to_le_bytes(), 0xeaff_fffeu32.to_le_bytes()].concat();
        let mut cpu = cpu_with(&code); // mov r0, #0x37 ; b .
        assert_eq!(cpu.run(0, 1).unwrap(), StopReason::Budget);
        assert_eq!(cpu.read_reg(Reg::R0), 0x37);
    }

    /// **Uma instrução exclusiva não pode derrubar o processo.**
    ///
    /// O emissor x64 do Dynarmic exige um `global_monitor` **no momento da tradução** de
    /// LDREX/STREX: `EmitExclusiveReadMemory` faz `ASSERT(conf.global_monitor != nullptr)` e depois
    /// o desreferencia (`emit_x64_memory.cpp.inc`). O invólucro nunca preencheu esse campo, e o
    /// crate 0.1.3 não tem setter (`a32.rs` faz `unsafe { std::mem::zeroed() }` com um `todo`), ou
    /// seja: o campo nasce nulo. Nada rebaixa essas instruções quando o monitor falta.
    ///
    /// 40 dos 62 `.mod` do acervo contêm esse padrão em algum lugar. O teste monta
    /// `ldrex r0, [r1]` seguido de `b .` e executa o bloco: sem monitor, a tradução aborta e o
    /// processo morre antes de a asserção ser lida.
    ///
    /// **Medido, e por isso ele fica ignorado em vez de verde**: em 24/09/2026 este teste matou o
    /// processo com `assertion failed: conf.global_monitor != nullptr` e `signal: 6, SIGABRT`. O
    /// conserto não é nosso — precisa de um patch no `dynarmic` 0.1.3 (dôr o campo
    /// `global_monitor` ao `Config` público), e o crate não tem setter. Com o patch, tirar o
    /// `#[ignore]` e este teste passa a ser a guarda.
    #[ignore = "prova um defeito conhecido da dependência; ver o comentário acima"]
    #[test]
    fn a_instrucao_exclusiva_nao_derruba_o_processo() {
        // `ldrex r0, [r1]` (0xE1910F9F) e `b .` (0xEAFFFFFE).
        let code = [0xe191_0f9fu32.to_le_bytes(), 0xeaff_fffeu32.to_le_bytes()].concat();
        let mut cpu = cpu_with(&code);
        // r1 aponta para a memória de dados, para a leitura ter um endereço mapeado.
        cpu.write_reg(Reg::R1, 0x1000);
        assert_eq!(cpu.run(0, 2).unwrap(), StopReason::Budget);
        assert_eq!(cpu.read_reg(Reg::R0), 0, "a memória começa zerada");
    }

    #[test]
    fn salto_para_a_faixa_de_api_vira_chamada() {
        let code = [0xe3a0_020fu32.to_le_bytes(), 0xe12f_ff10u32.to_le_bytes()].concat();
        let mut cpu = cpu_with(&code);
        assert_eq!(
            cpu.run(0, 10).unwrap(),
            StopReason::ApiCall { addr: API_BASE }
        );
    }

    #[test]
    fn endereco_com_bit_zero_entra_em_thumb() {
        // Em Thumb, `movs r0, #0x37` e depois `b .` — alinhado em 0x100. Chamado com 0x101,
        // que é como o despachante retoma um chamador Thumb.
        let mut code = vec![0u8; 0x100];
        code.extend_from_slice(&[0x37, 0x20, 0xfe, 0xe7]);
        let mut cpu = cpu_with(&code);
        assert_eq!(cpu.run(0x101, 1).unwrap(), StopReason::Budget);
        assert_eq!(cpu.read_reg(Reg::R0), 0x37);
    }

    #[test]
    fn endereco_par_volta_para_arm_depois_de_thumb() {
        // `mov r0, #0x37` em ARM no zero; em 0x100, código Thumb qualquer.
        let mut code = [0xe3a0_0037u32.to_le_bytes(), 0xeaff_fffeu32.to_le_bytes()].concat();
        code.resize(0x100, 0);
        code.extend_from_slice(&[0x00, 0x20, 0xfe, 0xe7]);
        let mut cpu = cpu_with(&code);
        cpu.run(0x101, 1).unwrap();
        assert_eq!(cpu.run(0, 1).unwrap(), StopReason::Budget);
        assert_eq!(cpu.read_reg(Reg::R0), 0x37);
    }

    #[test]
    fn a_vigia_liga_com_escrita_do_guest_e_nao_com_a_do_host() {
        // `mov r1, #0x1000 ; str r0, [r1] ; b .`
        let code = [
            0xe3a0_1a01u32.to_le_bytes(),
            0xe581_0000u32.to_le_bytes(),
            0xeaff_fffeu32.to_le_bytes(),
        ]
        .concat();
        let mut cpu = cpu_with(&code);
        cpu.watch_dirty(7, 0x1000, 4).unwrap();
        assert!(cpu.take_dirty(7), "nasce sujo");
        assert!(!cpu.take_dirty(7), "e ler limpa");
        cpu.write_mem(0x1000, &[1, 2, 3, 4]).unwrap();
        assert!(!cpu.take_dirty(7), "escrita do host não conta");
        cpu.run(0, 2).unwrap();
        assert!(cpu.take_dirty(7), "escrita do guest conta");
        // Uma faixa desarmada volta ao contrato padrão: na dúvida, sujo.
        cpu.unwatch_dirty(7);
        assert!(cpu.take_dirty(7));
    }

    #[test]
    fn contagem_de_callbacks_e_opt_in_e_separa_vigias_de_acessos_do_host() {
        let code = [0xe591_0000u32, 0xe582_0000, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut mem = GuestMemory::new();
        mem.map("code", 0, code, true).unwrap();
        mem.map_zeroed("data", 0x1000, PAGE as usize).unwrap();
        // Página parcial exercita uma leitura que precisa de callback mesmo com o fast path.
        mem.map_zeroed("partial", 0x4000, 4).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.watch_dirty(7, 0x1000, 8).unwrap();
        cpu.watch_dirty(8, 0x4000, 4).unwrap();
        cpu.take_dirty(7);
        cpu.take_dirty(8);
        cpu.write_reg(Reg::R1, 0x4000);
        cpu.write_reg(Reg::R2, 0x1004);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert!(cpu.relato_de_memoria().is_none());
        cpu.liga_contagem_de_memoria(true);
        cpu.write_mem(0x4000, &123u32.to_le_bytes()).unwrap();
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        let report = cpu.relato_de_memoria().unwrap();
        assert_eq!((report.leituras, report.escritas), (1, 1));
        assert_eq!((report.leituras_vigiadas, report.escritas_vigiadas), (1, 1));
        let mut bytes = [0; 4];
        cpu.read_mem(0x1004, &mut bytes).unwrap();
        assert_eq!(u32::from_le_bytes(bytes), 123);
        assert_eq!(cpu.relato_de_memoria().unwrap(), report);
        cpu.liga_contagem_de_memoria(false);
        assert!(cpu.relato_de_memoria().is_none());
        cpu.liga_contagem_de_memoria(true);
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 0);
    }

    #[test]
    fn leitura_direta_em_codigo_vigiado_conserva_a_escrita_interceptada() {
        let code = [0xe591_0000u32, 0xe581_2000, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.write_mem(0x80, &123u32.to_le_bytes()).unwrap();
        cpu.watch_dirty(7, 0x80, 4).unwrap();
        cpu.take_dirty(7);
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R1, 0x80);
        cpu.write_reg(Reg::R2, 456);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R0), 123);
        assert_eq!(cpu.read_u32(0x80).unwrap(), 456);
        assert!(cpu.take_dirty(7));
        let counts = cpu.relato_de_memoria().unwrap();
        assert_eq!(counts.escritas_em_codigo, 1);
        assert_eq!(counts.leituras, 0, "ler dados no código não deve sair do JIT");
    }

    #[test]
    fn leitura_direta_somente_leitura_preserva_falha_precisa_de_escrita() {
        let code = [0xe591_0000u32, 0xe581_2000, 0xe583_2000, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut mem = GuestMemory::new();
        mem.map("code", 0, code, true).unwrap();
        mem.map_zeroed("data", 0x1000, PAGE as usize).unwrap();
        let mut ro = vec![0; PAGE as usize];
        ro[..4].copy_from_slice(&123u32.to_le_bytes());
        mem.map("ro", 0x4000, ro, false).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R1, 0x4000);
        cpu.write_reg(Reg::R2, 456);
        cpu.write_reg(Reg::R3, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::MemoryFault { addr: 0x4000, pc: 4 });
        assert_eq!(cpu.read_reg(Reg::R0), 123);
        assert_eq!(cpu.read_u32(0x4000).unwrap(), 123);
        assert_eq!(cpu.read_u32(0x1000).unwrap(), 0);
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 1);
        assert_eq!(cpu.relato_de_memoria().unwrap().leituras, 0);
    }

    #[test]
    fn ponteiro_host_impar_nao_perde_um_byte_para_o_atributo_de_protecao() {
        let code = [0xe591_0000u32, 0xe581_2004, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut mem = GuestMemory::new();
        mem.map("code", 0, code, true).unwrap();
        // O vetor é alinhado, mas a página guest começa 4095 bytes após seu início.
        mem.map_zeroed("odd", 0x1001, 0x2000).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.write_mem(0x2000, &123u32.to_le_bytes()).unwrap();
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R1, 0x2000);
        cpu.write_reg(Reg::R2, 456);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R0), 123);
        assert_eq!(cpu.read_u32(0x2004).unwrap(), 456);
        assert_eq!(cpu.relato_de_memoria().unwrap().leituras, 1);
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 1);
    }

    #[test]
    fn leitura_ordenada_em_ro_desalinhada_no_host_conserva_fallback() {
        let code = [0xe191_0c9fu32, 0xe12f_ff1e] // lda r0,[r1]; bx lr
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut mem = GuestMemory::new();
        mem.map("code", 0, code, true).unwrap();
        let mut ro = vec![0; 0x2000];
        ro[0xffe..0x1002].copy_from_slice(&123u32.to_le_bytes());
        mem.map("misaligned-ro", 0x1002, ro, false).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R1, 0x2000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R0), 123);
        assert_eq!(cpu.relato_de_memoria().unwrap().leituras, 1,
            "um endereço guest alinhado não garante alinhamento do ponteiro host");
    }

    #[test]
    fn pagina_vigiada_suja_volta_ao_acesso_direto_e_rearma_quando_consumida() {
        let code = [0xe581_0000u32, 0xe581_0004, 0xe581_0008, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.watch_dirty(7, 0x1000, 12).unwrap();
        cpu.take_dirty(7);
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R0, 0x1234);
        cpu.write_reg(Reg::R1, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 1,
            "depois da primeira escrita, os próximos pixels não precisam de callback");
        assert!(cpu.take_dirty(7));
        cpu.run(0, 10).unwrap();
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 2, "consumir rearma a página");
        assert!(cpu.take_dirty(7));
        for at in [0x1000, 0x1004, 0x1008] { assert_eq!(cpu.read_u32(at).unwrap(), 0x1234); }
    }

    #[test]
    fn vigias_distintas_na_mesma_pagina_so_liberam_acesso_quando_todas_sujas() {
        let code = [0xe581_0000u32, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.watch_dirty(7, 0x1000, 4).unwrap();
        cpu.watch_dirty(8, 0x1008, 4).unwrap();
        cpu.take_dirty(7);
        cpu.take_dirty(8);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        cpu.write_reg(Reg::R1, 0x1000);
        cpu.run(0, 10).unwrap();
        assert!(!escrita_direta(&cpu, 1), "a outra faixa ainda está limpa");
        cpu.write_reg(Reg::R1, 0x1008);
        cpu.run(0, 10).unwrap();
        assert!(escrita_direta(&cpu, 1), "as duas faixas agora estão sujas");
        assert!(cpu.take_dirty(7));
        assert!(!escrita_direta(&cpu, 1));
        assert!(cpu.take_dirty(8));
        assert!(!cpu.take_dirty(7));
    }

    #[test]
    fn vigias_sobrepostas_continuam_sujando_todos_os_ids() {
        let code = [0xe581_0000u32, 0xe581_0004, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.watch_dirty(7, 0x1000, 8).unwrap();
        cpu.watch_dirty(8, 0x1004, 8).unwrap();
        cpu.take_dirty(7);
        cpu.take_dirty(8);
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R1, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        cpu.run(0, 10).unwrap();
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 2);
        assert!(cpu.jit().unwrap().vigias.borrow().iter().find(|v| v.0 == 8).unwrap().3,
            "um atalho da parte exclusiva não pode esconder a escrita na sobreposição");
        assert!(escrita_direta(&cpu, 1));
        assert!(cpu.take_dirty(7));
        assert!(cpu.take_dirty(8));
        cpu.unwatch_dirty(7);
        assert!(!escrita_direta(&cpu, 1));
        cpu.unwatch_dirty(8);
        assert!(escrita_direta(&cpu, 1));
    }

    #[test]
    fn pagina_executada_continua_interceptada_mesmo_com_vigia_suja() {
        let mut code = [0xe581_0000u32, 0xe591_2000, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        code.resize(PAGE as usize, 0);
        let mut cpu = cpu_with(&code);
        cpu.watch_dirty(7, 0x80, 4).unwrap();
        cpu.take_dirty(7);
        cpu.write_reg(Reg::R1, 0x80);
        cpu.write_reg(Reg::R0, 123);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        cpu.liga_contagem_de_memoria(true);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert!(!escrita_direta(&cpu, 0), "código nunca vira escrita sem invalidação");
        let counts = cpu.relato_de_memoria().unwrap();
        assert_eq!((counts.escritas_em_codigo, counts.leituras_em_codigo), (1, 0));
        assert_eq!(cpu.read_reg(Reg::R2), 123);
    }

    #[test]
    fn escrita_desalinhada_entre_pagina_liberada_e_vigia_limpa_intercepta_as_duas() {
        let mut mem = GuestMemory::new();
        let code = [0xe581_0000u32, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        mem.map("code", 0, code, true).unwrap();
        mem.map_zeroed("data", 0x1000, 0x2000).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.watch_dirty(7, 0x1ffe, 2).unwrap();
        cpu.watch_dirty(8, 0x2000, 2).unwrap();
        cpu.take_dirty(8);
        assert!(escrita_direta(&cpu, 1));
        assert!(!escrita_direta(&cpu, 2));
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R0, 0x1234_5678);
        cpu.write_reg(Reg::R1, 0x1ffe);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_u32(0x1ffe).unwrap(), 0x1234_5678);
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 1);
        assert!(escrita_direta(&cpu, 2), "a metade na página vizinha também sujou a vigia");
        assert!(cpu.take_dirty(7));
        assert!(cpu.take_dirty(8));
    }

    #[test]
    fn pagina_liberada_que_vira_codigo_rearma_escritas_e_invalida_o_bloco() {
        let code = [0xe581_0000u32, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.write_mem(0x1000, &[0xe3a0_2001u32, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
        cpu.watch_dirty(7, 0x1000, 8).unwrap();
        assert!(escrita_direta(&cpu, 1));
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0x1000, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R2), 1);
        assert!(!escrita_direta(&cpu, 1), "buscar código retira uma página antes liberada");
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R0, 0xe3a0_2002);
        cpu.write_reg(Reg::R1, 0x1000);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas_em_codigo, 1);
        assert_eq!(cpu.run(0x1000, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R2), 2, "a escrita guest recompila o bloco já executado");
        assert!(!escrita_direta(&cpu, 1));
    }

    #[test]
    fn pagina_parcial_nao_e_liberada_com_a_pagina_inteira_vizinha() {
        let mut mem = GuestMemory::new();
        let code = [0xe581_0000u32, 0xe581_0004, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        mem.map("code", 0, code, true).unwrap();
        mem.map_zeroed("data", 0x1000, 0x1004).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.watch_dirty(7, 0x1ffc, 8).unwrap();
        cpu.take_dirty(7);
        cpu.liga_contagem_de_memoria(true);
        cpu.write_reg(Reg::R0, 123);
        cpu.write_reg(Reg::R1, 0x1ffc);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert!(escrita_direta(&cpu, 1));
        assert!(cpu.tabela[2].is_null());
        assert_eq!(cpu.relato_de_memoria().unwrap().escritas, 2);
        assert_eq!(cpu.read_u32(0x2000).unwrap(), 123);
        // A palavra seguinte está fora da região: a folga do vetor não vira RAM guest.
        cpu.write_reg(Reg::R1, 0x2004);
        assert!(matches!(cpu.run(0, 10).unwrap(), StopReason::MemoryFault { addr: 0x2004, .. }));
    }

    #[test]
    fn vigia_suja_em_regiao_somente_leitura_nao_remove_a_protecao() {
        let mut mem = GuestMemory::new();
        let code = [0xe581_0000u32, 0xe582_0000, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        mem.map("code", 0, code, true).unwrap();
        mem.map_zeroed("data", 0x1000, PAGE as usize).unwrap();
        mem.map("ro", 0x4000, vec![0; PAGE as usize], false).unwrap();
        let mut cpu = DynarmicCpu::new().unwrap();
        cpu.reset(&mem).unwrap();
        cpu.watch_dirty(7, 0x4000, 4).unwrap();
        assert!(!escrita_direta(&cpu, 4));
        cpu.write_reg(Reg::R0, 123);
        cpu.write_reg(Reg::R1, 0x4000);
        cpu.write_reg(Reg::R2, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert!(matches!(cpu.run(0, 10).unwrap(), StopReason::MemoryFault { addr: 0x4000, .. }));
        assert_eq!(cpu.read_u32(0x1000).unwrap(), 0, "fault impede a escrita seguinte");
    }

    #[test]
    fn trocar_a_faixa_de_uma_vigia_rearma_so_a_nova_e_aceita_sujeira_explicita() {
        let code = [0xe581_0000u32, 0xe12f_ff1e]
            .into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.watch_dirty(7, 0x1000, 4).unwrap();
        cpu.take_dirty(7);
        cpu.watch_dirty(7, 0x1080, 4).unwrap();
        assert!(escrita_direta(&cpu, 1));
        assert!(cpu.take_dirty(7));
        cpu.write_reg(Reg::R1, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        cpu.run(0, 10).unwrap();
        assert!(!cpu.take_dirty(7), "a faixa antiga não pertence mais ao ID");
        cpu.marca_sujo(0x1080, 4);
        assert!(escrita_direta(&cpu, 1));
        assert!(cpu.take_dirty(7));
        assert!(!escrita_direta(&cpu, 1));
    }

    #[test]
    fn retorno_para_o_sentinela_e_reconhecido() {
        let mut cpu = cpu_with(&0xe12f_ff1eu32.to_le_bytes()); // bx lr
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
    }

    #[test]
    fn leitura_invalida_para_antes_da_proxima_escrita() {
        // LDR inválido seguido por STR válido no mesmo bloco: o fault deve ser preciso.
        let code = [0xe591_0000u32, 0xe583_2000, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.write_reg(Reg::R1, 0x3000);
        cpu.write_reg(Reg::R2, 0xdead_beef);
        cpu.write_reg(Reg::R3, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        let stop = cpu.run(0, 10).unwrap();
        let mut data = [0; 4];
        cpu.read_mem(0x1000, &mut data).unwrap();
        assert_eq!(
            data, [0; 4],
            "nenhuma escrita posterior ao fault pode acontecer"
        );
        assert_eq!(
            stop,
            StopReason::MemoryFault {
                addr: 0x3000,
                pc: 0
            }
        );
    }

    #[test]
    fn escrita_invalida_para_antes_da_proxima_escrita() {
        let code = [0xe581_0000u32, 0xe583_2000, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.write_reg(Reg::R1, 0x3000);
        cpu.write_reg(Reg::R2, 0xdead_beef);
        cpu.write_reg(Reg::R3, 0x1000);
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        let stop = cpu.run(0, 10).unwrap();
        let mut data = [0; 4];
        cpu.read_mem(0x1000, &mut data).unwrap();
        assert_eq!(
            data, [0; 4],
            "nenhuma escrita posterior ao fault pode acontecer"
        );
        assert_eq!(
            stop,
            StopReason::MemoryFault {
                addr: 0x3000,
                pc: 0
            }
        );
    }

    #[test]
    fn semihosting_escreve_sem_interromper_o_guest() {
        // mov r0,#SYS_WRITEC ; mov r1,#0x1000 ; svc #0xab ; bx lr
        let code = [0xe3a0_0003u32, 0xe3a0_1a01, 0xef00_00ab, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.write_mem(0x1000, b"Z").unwrap();
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.semihosting(), "Z");
        assert_eq!(cpu.read_reg(Reg::R0), 0);
    }

    /// O semihosting no estado ARM usa `SVC #0x123456`, e não pode parar o jogo.
    #[test]
    fn semihosting_do_estado_arm_tambem_escreve() {
        // mov r0,#SYS_WRITEC ; mov r1,#0x1000 ; svc #0x123456 ; bx lr
        let code = [0xe3a0_0003u32, 0xe3a0_1a01, 0xef12_3456, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let mut cpu = cpu_with(&code);
        cpu.write_mem(0x1000, b"C").unwrap();
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.semihosting(), "C");
    }

    #[test]
    fn codigo_alterado_pelo_host_e_recompilado() {
        let mut cpu =
            cpu_with(&[0xe3a0_0001u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat());
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R0), 1);

        // É o equivalente a uma API BREW atualizar uma tabela/rotina do módulo entre duas
        // chamadas. Sem `invalidate_cache_range`, o Dynarmic reutilizaria o `mov r0,#1`.
        cpu.write_mem(0, &0xe3a0_0002u32.to_le_bytes()).unwrap();
        cpu.write_reg(Reg::Lr, RETURN_MAGIC);
        assert_eq!(cpu.run(0, 10).unwrap(), StopReason::Returned);
        assert_eq!(cpu.read_reg(Reg::R0), 2);
    }
}
