# DSP FM para partituras SMAF

`ma_fm_core.cpp/.h` e `smaf_voice.cpp/.h` são cópias sem alterações de
[yamaha-smaf-player](https://github.com/akustikrausch/yamaha-smaf-player), de
Andreas Wendorf (Akustikrausch), commit `627f7ad8583ce34c5fec0269ef9778167af080d6`.
A licença Apache-2.0 original acompanha os fontes; `provenance.json` registra
os hashes. O binário do projeto já é distribuído sob GPLv3.

Somente DSP e conversão de instrumentos são usados. O parser, a agenda e os
limites continuam no Rust do emulador. `brew_smaf_fm.cpp` é a fronteira local:
48 vozes, 256 instrumentos, buffers verificados e exceções contidas. Não se
inclui janela, saída de áudio do host, ROM, SDK ou música de outro projeto.

Os instrumentos VMA carregados pelo jogo são interpretados pelo DSP. Para
presets sem parâmetros no arquivo, o banco FM do autor oferece timbres
substitutos; não são amostras Yamaha e não se afirma fidelidade acústica ou
temporal de ciclo de um chip histórico. A aceitação de formatos, notas,
tempos e controles é decidida pelo parser Rust estrito.
