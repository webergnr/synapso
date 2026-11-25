// Every string the page shows, in one place. A translation is a copy of this
// file with the same shape (see README › Localising).
//
// Claims here are checked against the app's README.md: if the app stops doing
// something, the sentence goes, not just the screenshot.

export const en = {
  meta: {
    title: 'Synapso — Markdown notes, canvases and the links between them',
    description:
      'A native desktop workspace for Markdown notes, JSON Canvas boards and a graph of the links between them. Plain files in a folder you own. Open source, Apache-2.0.',
    ogAlt: 'Synapso: think in notes, see the connections.',
  },

  nav: {
    label: 'Main',
    links: [
      { href: '#features', text: 'Features' },
      { href: '#canvas', text: 'Canvas' },
      { href: '#graph', text: 'Graph' },
      { href: '#download', text: 'Downloads' },
    ],
    github: 'GitHub',
    download: 'Download',
    menu: 'Menu',
    home: 'Synapso, home',
  },

  hero: {
    tags: ['Open source', 'Native desktop', 'Apache-2.0'],
    // rendered as: THINK IN NOTES. / SEE THE [[CONNECTIONS]]
    // (lineA breaks after its first part on phones)
    lineA: ['Think in', 'notes.'],
    lineB: 'See the',
    link: 'connections',
    lede:
      'A native workspace for Markdown notes, canvases and the links between them. Your vault is a folder of plain files: no account, no database, no lock-in.',
    primary: 'Download Synapso',
    secondary: 'View on GitHub',
    noRelease: 'See every release',
    triad: [
      { key: 'Fast', text: 'Native Rust, drawn with Skia. Frames in under 5 ms at p50.' },
      { key: 'Yours', text: 'Delete the app and the notes stay: they were never inside it.' },
      { key: 'Connected', text: 'Notes link with [[ ]], sit on canvases as cards, and meet in the graph.' },
    ],
    triadLabel: 'Synapso in three lines',
    figure: 'Exploded view of a Synapso vault: the folder on disk, a canvas, a Markdown note and the graph, stacked, with one note threaded through all four.',
    bomTitle: 'One note, four views',
    bom: [
      { part: 'graph', item: 'Graph', what: 'rings by hop', fmt: 'derived' },
      { part: 'note', item: 'Note', what: 'auth-flow.md', fmt: 'Markdown' },
      { part: 'canvas', item: 'Canvas', what: 'architecture.canvas', fmt: 'JSON Canvas 1.0' },
      { part: 'vault', item: 'Vault', what: '~/Synapso', fmt: 'a plain folder' },
    ],
  },

  proof: {
    id: 'app',
    title: 'The app, not a mockup.',
    lede:
      'Screenshots of Synapso itself, on the demo vault the repository generates in fixture/. Switch views the way the status bar does.',
    views: [
      { id: 'split', label: 'split', key: '⌘2', caption: 'A note and its canvas side by side. The divider drags; switching views glides.' },
      { id: 'notes', label: 'notes', key: '⌘1', caption: 'The editor alone. Markdown syntax folds away everywhere but the line you are on.' },
      { id: 'canvas', label: 'canvas', key: '⌘3', caption: 'The board alone: shapes, arrows, frames, pictures, and notes as cards.' },
      { id: 'graph', label: 'graph', key: '⌘G', caption: 'The note you are on in the middle, its links in rings around it, backlinks in the inspector.' },
      { id: 'picker', label: 'open', key: '⌘P', caption: 'The document picker: a folder column, the matches, and a preview of the row under the cursor.' },
      { id: 'light', label: 'light', key: '⌘⇧L', caption: 'Dark and light, from the same token file the design is drawn with.' },
    ],
    tablist: 'Views',
    alt: {
      split: 'Synapso in split view: the note auth-flow.md on the left, the canvas architecture.canvas on the right.',
      notes: 'Synapso in notes view: a Markdown guide note rendered as a document, with tabs across the top.',
      canvas: 'Synapso in canvas view: boxes, an ellipse and a triangle joined by arrows, and two note cards.',
      graph: 'Synapso graph view: auth-flow.md in the centre, linked notes in rings, backlinks listed in the inspector.',
      picker: 'The Synapso document picker open over the split view, filtered to the auth folder.',
      light: 'Synapso in the light theme.',
    },
  },

  markdown: {
    id: 'features',
    title: 'Type Markdown. Read a document.',
    body: [
      'The syntax that opens a line — ##, -, >, - [ ] — stays in the file and folds away everywhere but the line your caret is on. Nothing is converted, so nothing has to be converted back.',
      'Saving writes what is on screen, byte for byte. Syntax the editor has no style for comes back exactly as you typed it.',
    ],
    demoLabel: 'Try it: hover or tap a line to put the caret there.',
    demoFile: 'auth/auth-flow.md',
    demoFig: 'FIG. 02 · the editor, modelled',
    facts: ['Headings, lists, tasks, tables, code, quotes, footnotes', '[[wiki links]] and [[note#section]]', '⌘B ⌘I ⌘E and friends'],
  },

  canvas: {
    id: 'canvas',
    title: 'A canvas that is simple, and works.',
    body:
      'Shapes, arrows, frames, pictures and notes as cards. Drag a note from the tree and it becomes a card; rename it and every board that points at it is rewritten.',
    format:
      'A board is saved as JSON Canvas 1.0, the same format Obsidian’s canvas uses, so it opens there too. Keys another editor wrote are kept and handed back on save.',
    fileLabel: 'auth/architecture.canvas, on disk: the skeleton',
    toolsLabel: 'Tool rail',
    tools: [
      ['V', 'select'], ['R', 'rectangle'], ['O', 'ellipse'], ['Y', 'triangle'], ['A', 'arrow'],
      ['L', 'line'], ['P', 'pen'], ['T', 'text'], ['F', 'frame'],
    ],
    alt: 'The Synapso canvas: a client, an API gateway and an auth service joined by arrows, a token store, a retry triangle, and two note cards.',
  },

  graph: {
    id: 'graph',
    title: 'A graph you can read.',
    body:
      'Not a hairball of every note you own. The note you are on sits in the middle, what it links to sits in rings around it, one ring per hop, and the depth goes from 1 to 3.',
    inspector:
      'The inspector lists backlinks with their context, outgoing links, and notes that mention this one without linking it, with a button that writes the link.',
    // pins on the screenshot, in percent of its box, and what each one marks
    pins: [
      { x: 7.4, y: 15.5, label: 'Groups', text: 'top-level folders, each a toggle' },
      { x: 40.6, y: 38.4, label: 'Focus', text: 'the note you are on' },
      { x: 45.8, y: 26.8, label: 'Ring 1', text: 'one hop out' },
      { x: 51.4, y: 12.7, label: 'Beyond', text: 'past the depth: a dot' },
      { x: 85.8, y: 25.7, label: 'Backlinks', text: 'with their line' },
    ],
    alt: 'The Synapso graph: auth-flow.md in the centre, linked notes on rings by hop, and the inspector listing seven backlinks.',
  },

  keys: {
    id: 'keys',
    title: 'Hands stay on the keys.',
    body: 'Every view and every search has a shortcut, and the app spells them for your platform.',
    toggle: 'Show shortcuts for',
    mac: 'macOS',
    pc: 'Windows · Linux',
    rows: [
      { keys: '⌘K', what: 'Commands, and every document by name' },
      { keys: '⌘P', what: 'Open a document, with a preview' },
      { keys: '⌘⇧F', what: 'Search the text of the whole vault' },
      { keys: '⌘F', what: 'Find in the note; every match lights up' },
      { keys: '[[', what: 'Link to a note, or to a heading in it' },
      { keys: '⌘1 ⌘2 ⌘3', what: 'Notes, split, canvas' },
      { keys: '⌘G', what: 'The graph, on and off' },
      { keys: '⌘N', what: 'A new note, named in place' },
    ],
  },

  speed: {
    id: 'speed',
    title: 'Native, and it shows.',
    body:
      'Rust, drawn with Skia. The canvas is painted in one pass, only the visible lines of a note are laid out, nodes outside the view are culled, and text too small to read is skipped.',
    chartTitle: 'CPU time per frame, release build',
    scenario: 'Scenario',
    budget: '6.25 ms budget (160 Hz)',
    method:
      'Measured on Windows with a 160 Hz monitor, on a stress vault: a 3,121-line note and a 200-node, 259-edge canvas. Numbers from the README; SYNAPSO_STATS writes them.',
    caveat:
      'One case goes over: the p99 of a view transition, while the split width animates and the board and note relayout every frame.',
    rows: [
      { name: 'Canvas wheel-pan', p50: 2.0, p99: 2.8 },
      { name: 'Canvas marquee', p50: 2.4, p99: 3.3 },
      { name: 'Scroll the big note', p50: 2.4, p99: 3.0 },
      { name: 'Type in the big note', p50: 3.9, p99: 4.1 },
      { name: 'Notes/split/canvas transition', p50: 4.6, p99: 6.8 },
      { name: 'Graph (fixture vault)', p50: 1.8, p99: 4.4 },
    ],
  },

  files: {
    id: 'files',
    title: ['Your notes are files.', 'Not a subscription.'],
    lede: 'Your vault is an ordinary folder: no account, no database, no proprietary format. Delete Synapso and your notes are still there.',
    tree: [
      { path: '~/Synapso/', note: 'the vault: any folder, ⌘O picks another', depth: 0, color: 0 },
      { path: 'auth/', note: 'a folder is a group, with a colour', depth: 1, color: 1 },
      { path: 'auth-flow.md', note: 'a note: plain Markdown', depth: 2, color: 1 },
      { path: 'architecture.canvas', note: 'a board: JSON Canvas 1.0', depth: 2, color: 1 },
      { path: 'adapters/', note: '', depth: 1, color: 4 },
      { path: '.synapso.json', note: 'colours and recent files, kept out of your notes', depth: 1, color: 0 },
      { path: '.trash/', note: 'deleting moves here; it is not rm', depth: 1, color: 0 },
    ],
    rulesTitle: 'The file belongs to whoever wrote it',
    rules: [
      'A note is written 1.5 s after you stop typing, through a temporary file and a rename, so a crash mid-write truncates nothing.',
      'A note you did not edit is never written.',
      'If the file changed on disk since it was read, the write is refused and what you typed goes to a -unsaved file beside it.',
      'A file that could not be read opens empty, and its save is refused: an empty editor never becomes your file.',
      'Keys from a newer JSON Canvas or another editor are kept and handed back, instead of vanishing.',
    ],
  },

  download: {
    id: 'download',
    title: 'Download',
    latest: 'Latest release',
    published: 'published',
    unknown: 'Could not reach GitHub to read the latest release.',
    allReleases: 'All releases and notes',
    nightly: 'Nightly: the next version, can break',
    notAvailable: 'Not available yet',
    fromSource: 'Build from source',
    file: 'File',
    size: 'Size',
    get: 'Download',
    unsigned: {
      title: 'Not signed yet.',
      macos: 'macOS blocks the first launch. Open it once, then System Settings → Privacy & Security → Open Anyway (on macOS 14 and older: right-click → Open).',
      windows: 'SmartScreen warns on first launch: More info → Run anyway.',
    },
    yourSystem: 'your system',
    linuxNote: 'No Linux build is published, and CI does not build one. Building from source is the way in for now.',
    sourceTitle: 'Build from source',
    sourceNeeds: 'Needs Rust 1.94 or newer.',
    sourceCode: 'git clone https://github.com/webergnr/synapso\ncd synapso\ncargo install --path . --locked\nsynapso ~/notes',
    copy: 'Copy',
    copied: 'Copied',
  },

  oss: {
    id: 'source',
    title: 'Made by a developer, for developers.',
    body:
      'Synapso is open source under the Apache License 2.0. Read the code, file an issue, send a pull request. The design tokens the app is drawn with live in the same repository, and tests keep the two from drifting.',
    links: [
      { href: 'https://github.com/webergnr/synapso', text: 'Source on GitHub' },
      { href: 'https://github.com/webergnr/synapso/blob/main/CONTRIBUTING.md', text: 'Contributing' },
      { href: 'https://github.com/webergnr/synapso/issues', text: 'Issues' },
      { href: 'https://github.com/webergnr/synapso/blob/main/SECURITY.md', text: 'Security' },
    ],
    stackTitle: 'Built with',
    stack: [
      { name: 'Rust', href: 'https://www.rust-lang.org' },
      { name: 'Freya', href: 'https://github.com/marc2332/freya' },
      { name: 'Skia', href: 'https://skia.org' },
      { name: 'JSON Canvas', href: 'https://jsoncanvas.org' },
      { name: 'JetBrains Mono', href: 'https://github.com/JetBrains/JetBrainsMono' },
      { name: 'Excalifont', href: 'https://github.com/excalidraw/excalidraw' },
    ],
  },

  footer: {
    label: 'Footer',
    links: [
      { href: 'https://github.com/webergnr/synapso', text: 'GitHub' },
      { href: 'https://github.com/webergnr/synapso/releases', text: 'Releases' },
      { href: 'https://github.com/webergnr/synapso/blob/main/LICENSE', text: 'License' },
      { href: 'https://github.com/webergnr/synapso/blob/main/NOTICE', text: 'Notice' },
    ],
    license: 'Open source · Apache-2.0',
    fonts: 'Set in Archivo, JetBrains Mono and Excalifont (SIL OFL 1.1).',
  },
} as const;

export type Copy = typeof en;
