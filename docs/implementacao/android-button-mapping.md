# Mapeamento de botões Android

`getFilesDir()/button-mapping.json` contém um único mapa para todos os jogos. O Java
grava com `AtomicFile`, aceita o novo estado somente após a gravação, e o Rust carrega
uma cópia antes da sessão. Não há leitura desse arquivo no processamento de quadros
ou eventos. Arquivo ausente, JSON inválido ou versão desconhecida usam os padrões.

```json
{"version":1,"bindings":{"1":"key:96","left":"axis:0:-","soft1":"key:4:scan:158"}}
```

`bindings` guarda somente atribuições personalizadas. As chaves válidas, na ordem
compartilhada por `ButtonBindings.Target` e `Key::ALL`, são:

`0`, `1`, `2`, `3`, `4`, `5`, `6`, `7`, `8`, `9`, `*`, `#`, `up`, `down`, `left`,
`right`, `select`, `soft1`, `soft2`, `clear`.

Cada alvo recebe no máximo uma entrada personalizada. Atribuir a mesma entrada a
outro alvo remove a atribuição anterior. Uma entrada personalizada substitui o
destino padrão daquela entrada; as outras entradas padrão continuam disponíveis,
como no Pocket2Droid. Reset grava `{"version":1,"bindings":{}}`.

Tokens usam os valores nativos do Android:

- `key:N`: `KeyEvent.getKeyCode()` de 1 a 1023, sem os códigos reservados abaixo.
- `key:4:scan:S`: Back físico, com `getScanCode()` positivo. A captura exige também
  `getDeviceId() >= 0`; Back sem scan ou de dispositivo virtual continua navegação.
- `axis:A:+` e `axis:A:-`: direção positiva/negativa de um eixo `MotionEvent`.
  São aceitos X/Y (0/1), Z/RX/RY/RZ (11–14), HAT_X/HAT_Y (15/16), L/R trigger
  (17/18) e os aliases Gas/Brake (22/23). Gatilhos capturados usam a direção positiva.

O ID de dispositivo não é persistido: controles conectados depois continuam usando
as mesmas atribuições. O estado pressionado, por outro lado, é separado por dispositivo
e por entrada. Soltar Enter não solta o OK ainda mantido por Controller A, nem um toque
solto apaga a tecla física. Repetições de KeyDown não criam pressões adicionais.

Home (3), volume (24/25/164), Power (26), App Switch (187), Sleep/Wakeup (223/224)
ficam reservados ao Android, inclusive em arquivos editados manualmente. Menu (82)
continua Soft1 no DOOV, e Back físico com scan positivo continua Soft2. Isso não
transforma o botão/gesto Back do Android em uma tecla do jogo.

Os padrões preservam o frontend anterior: dígitos Android 7–16, Star/Pound 17/18,
D-pad 19–22, OK em 23/66/96, Soft1 em 1/102/82, Soft2 em 2/103 e Back físico,
Clear em 28/67. HAT_X/HAT_Y também fornecem o direcional de controles reais.
Manches e gatilhos podem ser atribuídos explicitamente.

`handset::receive_with_app` consulta as faixas reais de `InputDevice` uma vez por
dispositivo e conserva min/max/flat no mapa. Manches usam histerese de 0,65 para
pressionar e 0,35 para soltar, após normalizar a faixa e remover a zona morta.
HAT usa qualquer deslocamento do centro. A captura adapta o normalizador do
Pocket2Droid para esses mesmos tokens e exige neutralidade antes de capturar eixos
já mantidos. Perda de foco/menu deve chamar `release_all`; desconexão chama
`release_device`. `GameActivity` recebe a remoção/mudança de dispositivo via
`InputManager.InputDeviceListener` e enfileira o ID por JNI. O loop consome
`take_removed_devices()`, chama `release_device` e `apply` no teclado antes de
sincronizar a sessão. A consulta de faixas é refeita para um dispositivo removido
que reapareça. A integração também limpa o `TouchKeypad` e libera a sessão.

`ButtonMappingView` e `keypad_art::paint` usam os mesmos 35 PNGs copiados de
`Pocket2Droid/assets/buttons`. O desenho mantém Soft1 / direcional com OK / Soft2,
Clear sob Soft2, e as quatro linhas `123`, `456`, `789`, `*0#`. As texturas do
egui são decodificadas uma vez e conservadas no `Context`.
