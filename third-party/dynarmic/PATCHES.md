# Origem e alteração local

Cópia completa do crate crates.io `dynarmic` 0.1.3. SHA256 do arquivo `.crate`:
`5a4cc8717f33eb7b6703270a32bb8f00a580106521adf2775978260adef13723`.
O `.cargo_vcs_info.json` registra o commit da binding
`2bb0b7b242a16be9f030cc45f7a3c06ccd9775df`.
As licenças originais da binding, do C++ e dos externos permanecem nesta árvore.

Um setter público expõe o campo A32 `check_halt_on_memory_access`, já existente na binding
e na configuração C++. O layout FFI permanece o mesmo.
O core usa esse setter com `HaltReason::MemoryAbort` para que uma leitura/escrita
inválida não execute instruções posteriores do mesmo bloco. Os testes de regressão
em `src/cpu/dynarmic.rs` verificam o motivo, o PC e a ausência de escrita posterior.

No emissor A32→AArch64, `EmitA32CheckMemoryAbort` usava `LDAR X` para ler o
`volatile u32 halt_reason`. No vivo V2026 essa variável ficou em endereço terminado
em `0x774`: alinhado para u32, mas não para a leitura de 64 bits. A sonda de memória
inválida derrubou o processo com SIGBUS/BUS_ADRALN dentro do código JIT, repetidamente.
O patch usa `LDAR W`/`TST W`, como os outros testes de halt do mesmo backend.
Não modifica opções de otimização. O caminho A64 guest não é usado por este produto.

`build.rs` também acompanha mudanças em `dynarmic/src` e no CMakeLists, para que
um patch C++ não deixe o Cargo reutilizar uma biblioteca nativa anterior.

Não editar o cache global do Cargo: esta cópia versionada permite reproduzir o patch.

O perfil nativo do DOOV encontrou 12,89–14,06% dos ciclos do thread principal de BIA nos
componentes identificados da via de leitura, principalmente salvando/restaurando registradores
nos trampolins. O campo A32 opt-in `page_table_write_protect_bit`, desligado por padrão,
interpreta bit zero da entrada como escrita interceptada. Os emissores x64 e AArch64 testam
o atributo somente para stores; loads retiram os bits da máscara e usam o ponteiro normal.
Entradas nulas e acessos desalinhados que cruzam página mantêm o fallback. A64 guest conserva
seu comportamento anterior. Não há mudança de flags unsafe ou fastmem.

O booleano fica no padding anterior a `fastmem_pointer`, preservando os 368 bytes de UserConfig
verificados no wrapper C++. A binding Rust inclui o mesmo campo e valida máscara >=1 antes
de construir o JIT. O core só fornece ponteiros de oito bytes de alinhamento de páginas completas e estáveis; código,
regiões somente leitura e vigias limpas conservam o atributo até que seja seguro remover a
interceptação. Ponteiros desalinhados, páginas parciais e buracos continuam nas callbacks.
Assim não se aloca uma segunda tabela de 8 MiB. O efeito de desempenho exige A/B Android,
além das regressões de permissões, SMC, falhas precisas e vigias nos dois backends.

No AArch64 o desvio de proteção usa TST/B.NE: o fallback é emitido após o bloco inteiro e pode
ultrapassar os ±32 KiB de TBNZ. Leituras ordenadas pela page table verificam o endereço host
efetivo antes de LDAR para evitar falha de alinhamento. A sonda nativa também executa um bloco
de 2048 stores, LDA com mapeamento host desalinhado e com endereço guest desalinhado, além de
uma vigia em página de dados que nunca foi código.
