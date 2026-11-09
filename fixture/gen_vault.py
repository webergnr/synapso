"""Regenerate the POC fixture vault (`fixture/vault`, git-ignored) from `fixture/seed`.
Folders are groups; notes link with [[wikilinks]]; the sidecar holds folder colours."""
import json, os, shutil

HERE = os.path.dirname(os.path.abspath(__file__))
V1 = os.path.join(HERE, 'seed')
OUT = os.path.join(HERE, 'vault')

if os.path.exists(OUT):
    shutil.rmtree(OUT)
os.makedirs(OUT)
os.chdir(OUT)


def w(p, s):
    os.makedirs(os.path.dirname(p) or '.', exist_ok=True)
    open(p, 'w', encoding='utf-8', newline='\n').write(s.lstrip('\n'))


af = open(os.path.join(V1, 'auth-flow.md'), encoding='utf-8').read()
af = af.replace("[adapter-design.md](adapter-design.md)", "[[adapter-design]]")
af = af.replace("Session state lives in `TokenStore`", "Session state lives in `TokenStore` ([[token-store]])")
af += "\nSee [[gateway]] for routing, [[pkce]] for the verifier and [[tokens]] for rotation.\n"
w('auth/auth-flow.md', af)
w('auth/tokens.md', "# Tokens\n\nRefresh rotates on every use. Access tokens live 15 minutes.\n\nMinted by the flow in [[auth-flow]]; claims in [[jwt-claims]].\n")
w('auth/gateway.md', "# Gateway\n\nThe only component that sees the client secret. Routes every request.\n\nCalls [[auth-flow]] on a 401 and stores sessions via [[token-store]].\n")
w('auth/pkce.md', "# PKCE\n\nThe verifier is generated per login and never leaves the device.\n\nUsed by [[auth-flow]].\n")
w('auth/jwt-claims.md', "# JWT claims\n\n`sub`, `exp`, `scope`, `device`. Short-lived.\n\nRelated: [[tokens]], [[device-unlink]].\n")
w('auth/device-unlink.md', "# Device unlink\n\nRevoking a device kills its refresh token.\n\nSee [[jwt-claims]]. Mentioned in the offline grace discussion.\n")
w('adapters/adapter-design.md', "# Adapter design\n\nOne port trait per storage backend. The sled implementation is canonical; sqlite is a test double.\n\n## Ports\n\n- `TokenStore` — get, put, evict ([[token-store]])\n- `NoteStore` — read, write_checked, trash\n\nBackends: [[sled]], [[sqlite]].\n")
w('adapters/storage/sled.md', "# sled\n\nEmbedded, crash-safe. Trees per concern: `auth/v2`, `notes/v1`.\n\nInternals in [[sled-internals]].\n")
w('adapters/storage/sqlite.md', "# sqlite\n\nWAL mode, one writer. Only used in tests.\n\nMirrors [[sled]].\n")
w('adapters/storage/token-store.md', "# Token store\n\nA sled tree keyed by device id. Rebuilt on launch.\n\nBacked by [[sled]]; read by [[gateway]].\n")
w('adapters/storage/sled-internals.md', "# sled internals\n\nPage cache, log-structured writes, epoch-based reclamation.\n")
w('roadmap/roadmap.md', "# Roadmap\n\n## v0.4\n\n- [ ] Graph view\n- [ ] Design diff tool\n- [x] Freya POC\n\nReleases: [[release-v0.3]], [[release-v0.4]].\n")
w('roadmap/release-v0.3.md', "# Release v0.3\n\nHeadline feature: [[auth-flow]] with refresh rotation.\n")
w('roadmap/release-v0.4.md', "# Release v0.4\n\nFolders as groups, the graph view.\n\nPlanned in [[roadmap]].\n")
w('journal/2026-10-07.md', "# 2026-10-07\n\nPaired on the gateway retry. Thinking about 401 handling in [[auth-flow]].\n")
w('journal/sync-2026-09-12.md', "# Sync 2026-09-12\n\nAgreed to keep sqlite as a test double only. See [[sqlite]].\n")
w('inbox/ideas.md', "# Ideas\n\n- A timeline of edits per folder\n- Offline grace period setting\n")
w('inbox/reading-list.md', "# Reading list\n\n- Designing Data-Intensive Applications\n- The sled architecture notes\n")
w('inbox/scratch.md', "# Scratch\n\nnothing linked here yet.\n")
w('notes.md', "# Notes\n\nLoose thoughts. The [[auth-flow]] retry story still needs a diagram.\n")
w('README.md', "# Vault\n\nA fixture vault for the Freya POC: folders are groups, each with a colour.\n")

c = json.load(open(os.path.join(V1, 'architecture.canvas'), encoding='utf-8'))
for n in c['nodes']:
    if n.get('file') == 'auth-flow.md':
        n['file'] = 'auth/auth-flow.md'
    if n.get('file') == 'adapter-design.md':
        n['file'] = 'adapters/adapter-design.md'
    if n.get('shape') == 'caption':
        n['shape'] = 'label'
json.dump(c, open('auth/architecture.canvas', 'w', encoding='utf-8', newline='\n'), indent=2)
am = {"nodes": [
    {"id": "a", "type": "file", "x": 40, "y": 40, "width": 220, "height": 112, "file": "auth/auth-flow.md"},
    {"id": "b", "type": "file", "x": 300, "y": 120, "width": 220, "height": 96, "file": "adapters/storage/sqlite.md"},
    {"id": "c", "type": "file", "x": 80, "y": 230, "width": 220, "height": 96, "file": "auth/tokens.md"}], "edges": []}
json.dump(am, open('auth/auth-map.canvas', 'w', encoding='utf-8', newline='\n'), indent=2)
json.dump({"nodes": [{"id": "p", "type": "text", "x": 60, "y": 60, "width": 200, "height": 60, "text": "v0.4 planning"}], "edges": []},
          open('roadmap/v0.4-planning.canvas', 'w', encoding='utf-8', newline='\n'), indent=2)
prefs = {"colors": {"auth/": 0, "adapters/": 3, "roadmap/": 2, "journal/": 1, "inbox/": None, "inbox/markdown-guide": 6}}
json.dump(prefs, open('.synapso.json', 'w', encoding='utf-8', newline='\n'), indent=2)
os.makedirs('assets', exist_ok=True)
shutil.copy(os.path.join(V1, 'flow.png'), 'assets/flow.png')
w('roadmap/demo.md', """# Markdown demo

1. first step
2. second step
   - nested bullet

---

| name | role |
| --- | --- |
| sled | canonical |
| sqlite | test double |

![a screenshot from the design](assets/flow.png)

![missing picture](assets/nope.png)

See [[roadmap]].
""")
# every block and inline style the editor renders (the design's markdown guide)
w('inbox/markdown-guide.md', """# Markdown guide

Inline: **bold**, *italic*, ***bold italic***, ~~strikethrough~~, <u>underline</u>, `inline code`, ==highlight==, an [external link](https://example.com), a wiki link [[auth-flow]] and a footnote[^1].

## Headings

# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6

## Lists

### Unordered

- Gateway mints tokens
- Services trust short-lived JWTs
  - 15 minute expiry
  - Refresh on 401
- Client never sees the secret

### Ordered

1. Open the adapter
2. Run the contract tests
  1. read / write
  2. watch
3. Ship behind a flag

### Tasks

- [x] Draft adapter trait
- [ ] Benchmark sqlite reads
- [ ] Write migration notes

## Table

| adapter | status | p50 |
| :--- | :---: | ---: |
| sled | stable | 0.4 ms |
| sqlite | beta | 1.8 ms |
| memory | test only | 0.1 ms |

## Code

```ts
export interface Adapter {
  open(path: string): Promise<Handle>;
  read(id: DocId): Promise<Doc | null>;
  watch(fn: (e: Change) => void): () => void; // unsubscribe
}
```

## Quote

> Writes go through a single queue so the canvas never observes a half-written doc.
> > Nested quote: the queue is per vault, not per adapter.

## Image

![sequence diagram](sequence-diagram.png)

---

[^1]: Footnotes collect at the end of the note.
""")
print('vault regenerated')
