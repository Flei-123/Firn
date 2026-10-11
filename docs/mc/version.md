# mc_version / mc_sem / mc_versions -- the version object

See `docs/MC-VERSIONS.md` for the principle and the "add a version" checklist.

```
Version { protocol, flags, chunk_format, name, sem[96], id_limit(), name_of(), schema_of() }
mc_versions.version_by_protocol(n, &v) -> bool     all generated versions linked in one binary
mc_version.version_has(&v, FLAG_*)                 behaviour switches
mc_version.version_pid(&v, mc_sem.SEM_*)           wire id of a semantic packet, -1 = absent
v.name_of(state, dir, id) / v.schema_of(state, dir, id)   function values over the generated tables
```

Generated: `mc_sem.fi`, `mc_versions.fi`, `vNNN/mc_proto_vNNN.fi` (`python3 tools/mcgen/gen_protocol.py
--all <minecraft-data>/data lib/mc all`). Hand written facts per version: `tools/mcgen/versions.json`.
