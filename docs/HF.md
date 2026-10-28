# lib/hf -- Hugging Face hub + Range reader

- hf.hub: hf_list_configs(id, token, out), hf_list_files(id, config, split, token, out) (url, path, size per entry via the tree API, datasets-server fallback), hf_probe(url, token, size)
- hf.remote: hf_remote_open(url, token, out: HfSrc) -> read_at(ud, offset, dest, n) with the PqSrc layout (read_at, ud, size); window coalescing (default 512 KiB, tail aware), signed URL cached and re-resolved on 400/401/403/410 or a redirect, retries with backoff and Retry-After, resume of cut bodies at offset+got, cancel flag, counters
- hf_download(url, dest, token, resume) / hf_download_ex: net.download with .part resume, sha256 from X-Linked-ETag, re-resolve on expired URL
- Token: argument, else HF_TOKEN, else HUGGING_FACE_HUB_TOKEN. Authorization is sent only to the host of the given URL, never to the CDN host.
- Not thread safe per HfSrc. Windows: same code (TLS via net.http).
- Tests: tests/5722_hf_remote.fi (in-process mock), live checks in tools/hf/live.sh
