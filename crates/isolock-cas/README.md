# isolock-cas

Content-addressed storage primitives, SHA-256 digest validation, and streaming sharding for Isolock artifacts.

## Large artifacts

`shard_file(source, destination)` writes files larger than 95 MiB as ordered `.part000`, `.part001`, … files and emits a `<filename>.shards.json` manifest. Artifacts at or below the limit are left untouched. `read_shard_manifest` validates the manifest structure, and `verify_shards` checks each part digest plus the SHA-256 digest of their ordered concatenation.

After verification, POSIX `cat` reassembles the artifact without loading it all into memory:

```sh
cat "$shards_dir/$filename".part* > "$destination"
```
