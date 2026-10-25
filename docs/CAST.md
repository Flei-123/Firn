# CAST: wireless display

Core in `lib/cast/` is platform neutral Firn (no C, no C++, no Rust). Platform parts (capture, Wi-Fi Direct, sockets) stay outside the core.

## Feasibility (measured state, 25.10.2026)
- No H.264 encoder and no RTP/RTSP/MPEG-TS existed in `lib/`. Decoders exist only in Certus (`lib/media/h264.fi`) and OrientOS (`kernel/user/h264.fi`), not in Firn `lib/`.
- `net.udp` (UDP) and `std.net` (TCP) are enough for RTP and RTSP. `std.base64` is used for SDP.

## Modules
| module | purpose |
|---|---|
| `cast.bits` | bit writer / reader, Exp-Golomb (ue/se), RBSP stop bit |
| `cast.nal` | Annex B writer (emulation prevention), NAL iterator, unescape |
| `cast.yuv` | RGBA/BGRA <-> YUV 4:2:0 (BT.601 limited), macroblock copy / compare |
| `cast.h264enc` | screen-content H.264 encoder (Constrained Baseline) |
| `cast.h264dec` | decoder for the same subset (loopback, Firn sinks) |
| `cast.rtp` | RTP header, H.264 packetizer (RFC 6184, single NAL + FU-A), depacketizer with loss detection |
| `cast.rtcp` | SR, BYE, PLI (keyframe request) |
| `cast.pktlist` | packet list helper |
| `cast.mpegts` | MPEG-TS muxer (PAT, PMT, PES with PTS, PCR), demuxer, RTP payload type 33 (7 TS packets) |
| `cast.rtsp` | RTSP message parser / builder |
| `cast.sdp` | SDP for H.264 with `sprop-parameter-sets` |
| `cast.rtspsrv` | RTSP server state machine (OPTIONS, DESCRIBE, SETUP over UDP, PLAY, TEARDOWN), pure over octets |
| `cast.rtspcli` | RTSP client state machine, pure over octets |

## Encoder design
- Every stream is valid Constrained Baseline (profile 66, level by size), CAVLC, one slice per macroblock row, deblocking off, POC type 2, one reference frame.
- IDR: all macroblocks `I_PCM` (lossless). P frame: unchanged macroblocks are `P_Skip`, changed ones `I_PCM`. Tolerance `enc_set_tolerance` makes near-equal blocks skip (lossy in time).
- Cropping handles sizes that are not multiples of 16. Width and height are rounded up to even.
- ffmpeg decodes the output bit-identical to the encoder's reconstruction (`tools/cast/check.sh`, also as `.ts`).

## Open Firn-to-Firn path
Standard RTSP/1.0 + SDP over TCP, RTP/AVP over UDP (H.264, payload type 96), RTCP PLI for keyframe requests. Any RTSP player (ffplay, VLC) is a valid receiver; Windows, OrientOS and Android share the same Firn core. TLS for the control channel: `tls.tls` (not wired yet).

## Miracast (Wi-Fi Display) mapping
- Link: Wi-Fi Direct P2P group; source listens on TCP 7236; RTSP messages M1 to M7 (OPTIONS, GET/SET_PARAMETER, `wfd_trigger_method: SETUP`, SETUP, PLAY); media RTP/UDP payload type 33 (MPEG-TS with H.264, PCR/PTS). `cast.rtsp`, `cast.mpegts`, `cast.rtp` cover the wire formats. Missing: WFD parameter handling (`wfd_video_formats`, `wfd_client_rtp_ports`, `wfd_presentation_URL`) and the M1 to M7 state machines.
- HDCP/content protection: not supported (`wfd_content_protection: none`). Audio (LPCM/AAC) not implemented.

## Honest limits
- C1 I_PCM is lossless and large: a changed 16x16 block costs 390 bytes. Static desktops are tiny, full-screen video is not usable (1080p about 3 MB per full change). A transform coded P/I16x16 encoder (CAVLC tables) is the next codec step.
- C2 `cast.h264dec` decodes only this subset (I_PCM, P_Skip, Baseline, POC 2); other streams return `DEC_ERR_UNSUPPORTED`. A general sink needs the Certus/OrientOS decoder moved into `lib/`.
- C3 Colour: BT.601 limited, signalled in the VUI; no gamma handling.
- C4 No congestion control, no retransmission; loss marks the access unit damaged and the receiver should send PLI.
- C5 Level bitrate limits are not enforced.

## Hardware dependent (OrientOS kernel / drivers)
- Wi-Fi Direct (P2P GO negotiation, WPS PBC, P2P IE in probe/beacon, group formation) in the WLAN driver; the core only needs a TCP/UDP socket on the P2P interface.
- Screen capture source (framebuffer read / compositor tap) feeding `enc_encode`.
- Optional hardware H.264 (not needed for the subset).
- Windows: Wi-Fi Direct needs WinRT `Windows.Devices.WiFiDirect`; Android: `WifiP2pManager` / `MediaProjection`.

## Tests
`tests/5110` to `5114` (bits/NAL, encoder-decoder round trip, RTP over real UDP with loss, MPEG-TS mux/demux + RTP 33, RTSP/SDP session) and `tools/cast/check.sh` (ffmpeg oracle).
