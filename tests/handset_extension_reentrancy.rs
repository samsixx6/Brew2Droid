use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, ExtensionImage, modfile::ModImage},
    machine::{Machine, Outcome},
};
const OUTER: u32 = 0x010179be;
const INNER: u32 = 0x010179c1;
const BASE: u32 = loader::EXT_MODULE_BASE;
const OUTEROBJ: u32 = BASE + 0x440;
const INNEROBJ: u32 = BASE + 0x450;
fn write(words: &mut Vec<u32>, at: usize, vs: &[u32]) {
    while words.len() < at / 4 + vs.len() {
        words.push(0);
    }
    words[at / 4..at / 4 + vs.len()].copy_from_slice(vs);
}
fn image(inner_failure: bool) -> ModImage {
    let mut w = vec![0; 0x480 / 4];
    // AEEMod_Load writes the actual module whose slot2 is the ARM factory.
    write(
        &mut w,
        0,
        &[0xe59f3008, 0xe5823000, 0xe3a00000, 0xe12fff1e, BASE + 0x400],
    );
    write(&mut w, 0x400, &[BASE + 0x420]);
    write(&mut w, 0x420, &[0, 0, BASE + 0x100]);
    // Keep ppObj live across a real nested ISHELL_CreateInstance, matching Summus.
    write(
        &mut w,
        0x100,
        &[
            0xe92d4070, //push{r4,r5,r6,lr}
            0xe59f4044, //ldr r4,INNER
            0xe1520004, //cmp r2,r4
            0x0a00000b, //beq inner at0x140
            0xe59f403c, //ldr r4,OUTEROBJ
            0xe5834000, //str r4,[r3],before nested construction
            0xe1a00001, //mov r0,r1(shell)
            0xe59f102c, //ldr r1,INNER
            0xe59f2030, //ldr r2,INNEROUT
            0xe59fc030, //ldr ip,ISHELL_CreateInstance
            0xe12fff3c, //blx ip
            0xe3a00000,
            0xe8bd8070,
            0xe1a00000,
            0xe1a00000,
            0xe1a00000,
            0xe59f4018, //inner:ldr r4,INNEROBJ
            0xe5834000,
            0xe3a00000,
            0xe8bd8070,
            INNER,
            OUTEROBJ,
            BASE + 0x460,
            aee::encode(Interface::Shell, 2),
            INNEROBJ,
        ],
    );
    if inner_failure {
        w[0x140 / 4] = 0xe3a04000;
        w[0x148 / 4] = 0xe3a00001;
    }
    ModImage::parse(w.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap()
}
fn fixture(inner_failure: bool) -> Machine<BackendPadrao> {
    let main = ModImage::parse(
        [0xe3a00000u32, 0xe12fff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let loaded = loader::load_with(
        &main,
        &[ExtensionImage {
            image: image(inner_failure),
            classes: vec![OUTER, INNER],
        }],
    )
    .unwrap();
    let mut m = Machine::new(BackendPadrao::new().unwrap(), loaded, ".");
    assert_eq!(m.run(10000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn create(m: &mut Machine<BackendPadrao>) -> Outcome {
    let shell = m.module().shell;
    let out = loader::HEAP_BASE + 0x1000;
    m.call_guest(
        aee::encode(Interface::Shell, 2),
        [shell, OUTER, out, 0],
        10000,
    )
    .unwrap()
}
#[test]
fn fabrica_aninhada_preserva_ppobj_da_fabrica_externa() {
    let mut m = fixture(false);
    let sp = m.cpu().read_reg(zeebx::cpu::Reg::Sp);
    assert_eq!(create(&mut m), Outcome::Returned { code: 0 });
    assert_eq!(m.cpu().read_u32(BASE + 0x460).unwrap(), INNEROBJ);
    assert_eq!(
        m.cpu().read_u32(loader::HEAP_BASE + 0x1000).unwrap(),
        OUTEROBJ
    );
    assert_eq!(m.cpu().read_reg(zeebx::cpu::Reg::Sp), sp);
}
#[test]
fn falha_interna_nao_apaga_ppobj_externo_e_restaura_pilha() {
    let mut m = fixture(true);
    let sp = m.cpu().read_reg(zeebx::cpu::Reg::Sp);
    assert_eq!(create(&mut m), Outcome::Returned { code: 0 });
    assert_eq!(m.cpu().read_u32(BASE + 0x460).unwrap(), 0);
    assert_eq!(
        m.cpu().read_u32(loader::HEAP_BASE + 0x1000).unwrap(),
        OUTEROBJ
    );
    assert_eq!(m.cpu().read_reg(zeebx::cpu::Reg::Sp), sp);
}
#[test]
fn fabrica_recusa_pilha_sem_espaco_para_saida_sem_alterar_sp() {
    let mut m = fixture(false);
    let sp = loader::STACK_BASE + 4;
    m.cpu_mut().write_reg(zeebx::cpu::Reg::Sp, sp);
    assert_eq!(create(&mut m), Outcome::Returned { code: 20 });
    assert_eq!(m.cpu().read_u32(loader::HEAP_BASE + 0x1000).unwrap(), 0);
    assert_eq!(m.cpu().read_reg(zeebx::cpu::Reg::Sp), sp);
}

#[test]
fn fabrica_tambem_preserva_pilha_propria_alocada_na_heap() {
    let mut m = fixture(false);
    let malloc = aee::encode(Interface::Helpers, 26);
    let Outcome::Returned { code: stack } = m.call_guest(malloc, [8192, 0, 0, 0], 10000).unwrap()
    else {
        panic!("malloc");
    };
    assert_ne!(stack, 0);
    let sp = stack + 8192;
    m.cpu_mut().write_reg(zeebx::cpu::Reg::Sp, sp);
    assert_eq!(create(&mut m), Outcome::Returned { code: 0 });
    assert_eq!(
        m.cpu().read_u32(loader::HEAP_BASE + 0x1000).unwrap(),
        OUTEROBJ
    );
    assert_eq!(m.cpu().read_reg(zeebx::cpu::Reg::Sp), sp);
}
#[test]
fn orcamento_da_fabrica_aninhada_restaura_sp_sem_prometer_continuacao() {
    let mut m = fixture(false);
    let sp = m.cpu().read_reg(zeebx::cpu::Reg::Sp);
    m.cpu_mut()
        .write_mem(BASE + 0x140, &0xeafffffeu32.to_le_bytes())
        .unwrap();
    // A fábrica externa ignora a falha interna e conserva seu objeto válido.
    // O laço interno não ganha continuação: só a saída e a pilha são isoladas.
    assert_eq!(create(&mut m), Outcome::Returned { code: 0 });
    assert_eq!(
        m.cpu().read_u32(loader::HEAP_BASE + 0x1000).unwrap(),
        OUTEROBJ
    );
    assert_eq!(m.cpu().read_reg(zeebx::cpu::Reg::Sp), sp);
}
