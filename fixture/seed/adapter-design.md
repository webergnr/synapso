# Adapter design

One port trait per storage backend. The sled implementation is canonical; sqlite is a test double.

## Ports

- `TokenStore` — get, put, evict
- `NoteStore` — read, write_checked, trash
