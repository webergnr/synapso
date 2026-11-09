# ADR-008 — a nota é um buffer só

**Estado:** aceito — é o editor · **Data:** 2026-08-19

## Contexto

O editor de blocos dá a cada bloco o seu próprio `text_editor`. O iced escopa
seleção, arrasto e cursor ao widget que os possui, então **atravessar um bloco é
uma ponte que alguém escreve e conserta para sempre**. Em um único dia saíram
três bugs, todos da mesma família:

- arrasto do mouse não passava de um bloco (PR #68)
- o teclado não acompanhava o arrasto, e o último bloco entrava no ⌘C sem
  aparecer selecionado (PR #70)
- ⌘A + clique deixava a nota picotada, porque `clear_selection` entregava ao
  iced um `selection: None` que ele ignora (PR #70)
- Shift+seta andava uma linha dentro do bloco e o bloco inteiro ao cruzar (PR #70)
- o contador `Ln` da barra de status só via as linhas do bloco focado

Nenhum é um bug de lógica. Todos são a mesma decisão de arquitetura cobrando.

## Decisão

A nota inteira é **um** `text_editor`, segurando o markdown **cru**. O que era
bloco virou *tipo de linha*, derivado do texto a cada edição — nada guardado em
dois lugares, nada para sair de sincronia.

O markdown que abre a linha (`## `, `- `, `> `, `- [ ] `) continua no buffer,
onde o arquivo precisa dele, e é desenhado **em tamanho nenhum**: invisível e sem
largura. O marcador, o número e a caixa de seleção são pintados no espaço que ele
deixa, por canvas empilhados com o editor — um **embaixo** (a imagem, a caixa do
código e a da tabela: qualquer coisa preenchida apagaria o texto se fosse por
cima) e um **em cima** (marcadores, spine, régua, legenda).

Uma imagem é o caso extremo da mesma ideia: a linha `![alt](src)` é **toda**
sintaxe, então some inteira e a altura da linha passa a ser a altura da figura —
a figura é pintada na fresta que sobrou, com o `alt` de legenda embaixo. Nada de
caixa inline: é o `line_height` do próprio span que reserva o espaço.

Isso exige `Format::size` no `Highlighter`, que o iced 0.14 não tem. O
`Cargo.toml` traz os três crates de um fork
([`webergnr/iced`, branch `0.14.0-per-span-font-size`](https://github.com/webergnr/iced/tree/0.14.0-per-span-font-size)):
o 0.14.0 com `Format::size`, mais o conserto da geometria de seleção e cursor,
que multiplicava índice de linha por uma altura uniforme. PR upstream pendente.

## O que isso resolve sozinho

Seleção entre blocos, ⌘A, arrasto, Shift+seta e o contador de linha **deixam de
ter código**. São do widget de novo. O `Ln 3` da barra é a terceira linha do
arquivo porque é literalmente a terceira linha do buffer.

E o arquivo passa a ser o buffer: salvar não re-emite a partir de uma árvore, então
o que o escritor digitou é o que o disco recebe, byte a byte. ⌘Z também: o passo
guardado é o texto do buffer, não a re-emissão do `Body` — desfazer por ali
devolveria uma cópia normalizada (`*` vira `-`, uma lista que começa em 3 é
renumerada) de uma nota que ninguém digitou.

## O que veio junto quando o editor de blocos saiu (2026-08-19)

A flag caiu e `editor.rs` perdeu a metade que desenhava um `text_editor` por
bloco. O que aquele editor sabia fazer e o buffer não sabia foi portado, não
descartado:

- **Tab / ⇧Tab** anda pelas células da tabela (crescendo uma linha ao sair da
  última) e aninha um item de lista, com a mesma regra de antes: só encosta um
  nível abaixo de um item que já existe.
- **⇧Enter** sai de uma cerca; em qualquer outro lugar é a quebra de linha que
  o widget faria sozinho.
- **Backspace onde começam as palavras** come o marcador inteiro. Você nunca viu
  o `- [ ] ` na tela, então tirá-lo caractere a caractere deixaria `- [ task`.
- **`[[`** continua abrindo a paleta de menções — a única regra de entrada que
  sobrou, porque todo o resto do markdown é literal no buffer e não precisa
  virar nada.
- **Mudar o tipo da linha** (barra de ferramentas, menu) agora *escreve a
  sintaxe* no arquivo. `Image`, `Table`, `Code` e `Rule` não são oferecidos: a
  sintaxe deles não é um prefixo, não há o que escrever na frente da linha.
- **⇥ dentro de uma cerca** digita um tab: ali é código, e é o que a tecla do
  widget faria. Fora dela, o caractere no arquivo nunca é o que alguém queria.
- **⇧Enter numa linha de tabela** abre a próxima linha, como abria no editor de
  blocos — uma quebra ali corta a linha no meio e `| c ` deixa de ser uma linha.
- **Enter numa cerca que abre e não fecha** escreve o fechamento, com a mesma
  cerca que foi digitada. Sem isso o resto do arquivo inteiro vira código.
- **Colar** é colar — os bytes da área de transferência são o que o arquivo
  recebe. A única exceção é uma linha que é só a sintaxe que a abre (`- ` e nada
  mais): colar um documento ali substitui a linha. Por isso **`colar como texto
  simples` saiu do menu**: fazia exatamente o mesmo que colar.
- **⌘A, ⌘C, ⌘X** voltaram a ser do widget. Não há mais seleção entre editores
  para manter, nem `select_all` segurando a nota.

E o ⌘F passou a apontar para uma **linha do arquivo** em vez de um bloco de uma
árvore — que é o que a nota é agora.

⚠️ **Tudo que mexe numa grade pergunta `table::is_whole` antes.** O `align`
reescreve a linha 1 como divisor sem perguntar, e `Kind::Table` é só "começa e
termina com pipe" — sem a pergunta, ⇥ em `| a | b |` / `| c | d |`, que é o
estado normal de uma tabela sendo digitada, **substituía a segunda linha por
traços**.

⚠️ **`Position::column` do iced é um offset em BYTES**, não uma contagem de
caracteres (é o `Cursor::index` do cosmic-text repassado). Havia um comentário
neste código afirmando o contrário; com ASCII os dois coincidem, e foi
`| Peça | Qtd |` que separou os dois — o `ç` gasta dois bytes e a seleção parava
antes do último `|`.

## O preço

- **O marcador ocupa posição de cursor sem ocupar espaço.** Um clique na primeira
  palavra caía *antes* dele e digitar ali quebrava o tipo da linha; hoje o cursor
  é empurrado para depois do marcador (`snap_out_of_marker`). Com as setas ainda
  dá para entrar nele — a coluna anda por um caractere que não se vê.
- **Um reparse por tecla.** `doc::Lines` é o modelo que busca, canvas,
  contagem de palavras e cartões leem, e é re-derivado do texto a cada edição
  (o `Body` do editor iced, com seu serializador, saiu em 2026-08-27).
  Correto e sem segundo caminho de sincronia, mas O(n) por tecla. Medir com uma
  nota grande antes de otimizar.
- **O cursor pode parar dentro de uma linha inteiramente escondida** — imagem e
  régua. O clique é empurrado para fora do marcador, a seta não: parado ali, o
  cursor não aparece. A figura ganha um anel quando o cursor está na linha dela,
  que é o que diz onde se está; a régua não tem equivalente.
- **O frontmatter está à vista.** O buffer é o arquivo, então o `---` … `---` do
  topo aparece na nota. Quem lê a nota (busca, cartões, ⌘K) continua sem vê-lo;
  quem edita pode quebrá-lo. Era invisível no editor de blocos.

## Emenda 2026-08-19 — o inline sem a sintaxe

Pedido: que a nota se pareça com o Raycast — `código` num chip, ~~riscado~~ com
risco de verdade, citação aninhada com uma barra por nível, e a ênfase sem os
asteriscos em volta.

**O que passou a valer:**

- **O marcador some**, do mesmo jeito que a sintaxe que abre a linha: fica no
  buffer, desenhado em `HIDDEN`. Ele **volta na linha em que o cursor está** —
  sintaxe que não se vê é sintaxe que não se edita, e essa é justamente a linha
  que está sendo editada. Custo: mover o cursor entre linhas re-highlighta a
  nota. É o mesmo custo que digitar já pagava (uma edição gera um `Arc` novo,
  que falha o `ptr_eq` e re-highlighta tudo); mover dentro da linha é de graça.
- **O chip e o risco são pintados**, não estilizados: `Format` carrega cor e
  fonte, e nenhum dos dois é fundo nem tachado. Vão nas canvases empilhadas com
  o editor, medidos em `size * 0.6` — a mesma coluna monoespaçada com que
  `Gutter::rows` calcula a quebra, que é o que impede os dois de divergirem.
- **Citação conta os níveis.** `> > >` são três, uma barra cada. O prefixo é
  desenhado **transparente mas em tamanho cheio**: a largura dele é o que
  degrau a degrau empurra cada nível para a direita. É a única exceção ao
  "marcador não ocupa espaço", e `Line::lead` é quem a declara.

## ⚠️ O teto que apareceu: uma linha, uma fonte

**Negrito e itálico nunca foram desenhados nesta nota** — o que se via era o
`**` e o `*`, não o peso. Não é bug nosso: o cosmic-text resolve a face **uma
vez por *run* de shaping**, a partir dos atributos do primeiro byte do run
(`shape_run`, cosmic-text 0.15), enquanto **cor e métrica ele lê por glifo**.

É por isso que o `Format::size` do nosso fork funciona e o `Format::font`
nunca funcionou dentro de uma linha: um heading fica em negrito porque a linha
**inteira** pede negrito; `**isto**` não fica, porque a linha em volta pede
regular.

Consequência prática: esconder `*` e `**` apagaria a ênfase em vez de desenhá-la.
Então **eles ficam**, e só some a sintaxe que tem outro sinal na tela (o chip, o
risco, a cor do link). `inline::vanishes` é a regra, e some quando isto for
resolvido.

**As três saídas** (todas são decisão sobre o fork, ver `synapso-iced-fork`):

1. **Fonte variável.** O peso por glifo *é* aplicado — como eixo `wght`
   (`swash.rs`). Os TTFs estáticos que embarcamos ignoram; a JetBrains Mono
   publica uma variável. Resolve o negrito, não o itálico.
2. **`CacheKeyFlags::FAKE_ITALIC`** é lido **por glifo** e é uma inclinação de
   14° no swash. Plumbar isso pelo `Format` no nosso fork do iced resolve o
   itálico sem fonte nova.
3. **Fork do cosmic-text** para quebrar o run na fronteira de face. É o conserto
   certo e o que o upstream aceitaria — e é um segundo fork para manter vivo.

## Como testar

```sh
cargo run
```
