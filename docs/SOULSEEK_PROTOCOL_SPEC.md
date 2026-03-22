# Soulseek Protocol Specification & NX Player Integration Plan

> Compiled from reverse-engineering analysis of Nicotine+ (Python, GPL-3.0),
> slskd/Soulseek.NET (C#, GPL-3.0), and the official protocol documentation
> maintained by the Nicotine+ project (`doc/SLSKPROTOCOL.md`).
>
> Last updated: 2026-03-22

---

## Table of Contents

1. [Protocol Overview](#1-protocol-overview)
2. [Binary Encoding](#2-binary-encoding)
3. [Connection Architecture](#3-connection-architecture)
4. [Server Messages (Essential Subset)](#4-server-messages-essential-subset)
5. [Peer Init Messages](#5-peer-init-messages)
6. [Peer Messages (Essential Subset)](#6-peer-messages-essential-subset)
7. [File Messages](#7-file-messages)
8. [Distributed Search Network](#8-distributed-search-network)
9. [Complete Download Flow](#9-complete-download-flow)
10. [Streaming Strategy for NX Player](#10-streaming-strategy-for-nx-player)
11. [Rust Implementation Architecture](#11-rust-implementation-architecture)
12. [Legal Considerations](#12-legal-considerations)

---

## 1. Protocol Overview

Soulseek is a proprietary P2P file-sharing network. The protocol has never been
officially documented; everything below comes from reverse engineering.

### Network Topology

```
                    +-------------------+
                    |  server.slsknet.org|
                    |     port 2242     |
                    +--------+----------+
                             |
              Server connection (TCP)
              (login, search dispatch,
               peer address lookup,
               distributed network mgmt)
                             |
         +-------------------+-------------------+
         |                                       |
    +---------+                            +---------+
    | Peer A  |------- P connection -------| Peer B  |
    |         |------- F connection ----->>|         |
    |         |------- D connection -------| (child) |
    +---------+                            +---------+

    P = Peer messages (chat, search results, transfer negotiation)
    F = File transfer (raw data after init handshake)
    D = Distributed search network (tree-structured search propagation)
```

### Four Connection Types

| Type | Purpose | Framing | Notes |
|------|---------|---------|-------|
| **S** (Server) | Login, search dispatch, peer lookup | `[len:u32][code:u32][payload]` | Single persistent TCP connection |
| **P** (Peer) | Search responses, transfer negotiation, user info | `[len:u32][code:u32][payload]` | One per peer, reused |
| **F** (File) | Actual file data transfer | `[token:u32]` then `[offset:u64]` then raw bytes | One per transfer |
| **D** (Distributed) | Search request propagation | `[len:u32][code:u8][payload]` | Tree topology |

---

## 2. Binary Encoding

All integers are **little-endian**. Strings are **length-prefixed** (not null-terminated).

### Primitive Types

| Type | Size | Encoding |
|------|------|----------|
| `uint8` | 1 byte | unsigned |
| `uint16` | 2 bytes | little-endian unsigned (rare, only in GetPeerAddress) |
| `int32` | 4 bytes | little-endian signed |
| `uint32` | 4 bytes | little-endian unsigned |
| `uint64` | 8 bytes | little-endian unsigned |
| `bool` | 1 byte | 0 = false, 1 = true |
| `string` | 4 + N bytes | `[length:uint32][utf8_bytes:N]` |
| `ip` | 4 bytes | 4 octets in **reverse** byte order (i.e., network order reversed) |

### String Encoding

```
pack_string("hello"):
  05 00 00 00   (length = 5, little-endian uint32)
  68 65 6c 6c 6f  ("hello" in UTF-8)
```

Modern clients use UTF-8. Legacy clients (Soulseek NS) used Latin-1.
When parsing, try UTF-8 first, fall back to Latin-1.

### IP Address Encoding

IP addresses are packed as 4 bytes with **reversed** byte order relative to
standard network order. To unpack: read 4 bytes, reverse them, then interpret
as a dotted-quad IPv4 address.

```python
# Nicotine+ approach:
inet_ntoa(message[start:start+4][::-1])
```

---

## 3. Connection Architecture

### Server Connection

- **Host**: `server.slsknet.org`
- **Port**: `2242`
- **Protocol**: TCP
- **Keepalive**: TCP keepalive or ServerPing (code 32) every ~60 seconds
- **Reconnect**: Exponential backoff on disconnection

### Message Framing (Server & Peer)

```
+------------------+------------------+------------------+
| Message Length    | Message Code     | Message Payload  |
| (uint32)         | (uint32)         | (variable)       |
+------------------+------------------+------------------+
|<-------- Message Length includes code + payload ------->|
```

**Important**: `Message Length` = `sizeof(code) + sizeof(payload)` = `4 + len(payload)`.
The length field itself is NOT included in the count.

### Message Framing (Distributed)

Same structure but the code is `uint8` instead of `uint32`:

```
+------------------+------------------+------------------+
| Message Length    | Message Code     | Message Payload  |
| (uint32)         | (uint8)          | (variable)       |
+------------------+------------------+------------------+
```

### Message Framing (File)

File messages have NO code field. The `F` connection uses a fixed sequence:
1. Uploader sends `FileTransferInit` (just a uint32 token, no length prefix from our perspective -- but it IS length-prefixed at the wire level like `P` init messages)
2. Downloader responds with `FileOffset` (just a uint64 offset)
3. Then raw file bytes flow until the connection closes

### Peer Init Message Framing

Peer init messages use `uint8` for the code:

```
+------------------+------------------+------------------+
| Message Length    | Init Code        | Init Payload     |
| (uint32)         | (uint8)          | (variable)       |
+------------------+------------------+------------------+
```

---

## 4. Server Messages (Essential Subset)

### Code 1: Login

**Send:**
```
[string username]
[string password]
[uint32 version]        // 160 for Nicotine+
[string md5_hash]       // MD5 hex digest of (username + password) concatenated
[uint32 minor_version]  // 1
```

**Receive (success):**
```
[bool   success]         // true
[string banner]          // MOTD
[ip     own_ip]          // 4 bytes, reversed
[string password_hash]   // MD5 hex digest of password alone
[bool   is_supporter]    // donated to Soulseek
```

**Receive (failure):**
```
[bool   success]           // false
[string rejection_reason]  // "INVALIDPASS", "INVALIDUSERNAME", etc.
[string detail]            // optional, only for INVALIDUSERNAME
```

### Code 2: SetWaitPort

**Send:**
```
[uint32 port]            // listening port (default 2234)
```

No response. Must be sent immediately after login.

### Code 3: GetPeerAddress

**Send:** `[string username]`

**Receive:**
```
[string username]
[ip     ip_address]      // 4 bytes reversed
[uint32 port]
[uint32 obfuscation_type]
[uint16 obfuscated_port]
```

### Code 5: WatchUser

**Send:** `[string username]`

**Receive:**
```
[string username]
[bool   exists]
[uint32 status]          // 0=offline, 1=away, 2=online
[uint32 avgspeed]        // bytes/sec
[uint32 uploadnum]
[uint32 unknown]
[uint32 files]           // shared file count
[uint32 dirs]            // shared directory count
[string countrycode]     // if status != offline
```

### Code 18: ConnectToPeer

Used for indirect peer connections (NAT traversal via server relay).

**Send:**
```
[uint32 token]
[string username]
[string conn_type]       // "P", "F", or "D"
```

**Receive:**
```
[string username]
[string conn_type]
[ip     ip_address]
[uint32 port]
[uint32 token]
[bool   privileged]
[uint32 obfuscation_type]
[uint32 obfuscated_port]
```

### Code 26: FileSearch

**Send:**
```
[uint32 token]           // client-generated, used to match responses
[string search_query]
```

**Receive (when another user searches, forwarded by server for UserSearch/RoomSearch):**
```
[string username]        // who is searching
[uint32 token]
[string search_query]
```

### Code 35: SharedFoldersFiles

**Send:**
```
[uint32 num_dirs]
[uint32 num_files]
```

### Code 36: GetUserStats

**Send:** `[string username]`

**Receive:**
```
[string username]
[uint32 avgspeed]
[uint32 uploadnum]
[uint32 unknown]
[uint32 files]
[uint32 dirs]
```

### Code 71: HaveNoParent

**Send:**
```
[bool have_no_parent]    // true = we need a parent in distributed network
```

### Code 93: EmbeddedMessage

**Receive (from server when we are a branch root):**
```
[uint8  distrib_code]
[bytes  distrib_message]
```

The server sends us distributed search messages to propagate to our children.

### Code 100: AcceptChildren

**Send:**
```
[bool accept]            // whether we can accept child peers
```

### Code 102: PossibleParents

**Receive:**
```
[uint32 num_parents]
iterate:
  [string username]
  [ip     ip_address]
  [uint32 port]
```

### Code 1001: CantConnectToPeer

**Send:**
```
[uint32 token]
[string username]
```

**Receive:**
```
[uint32 token]
```

---

## 5. Peer Init Messages

### Init Code 0: PierceFireWall

Sent to a peer in response to an indirect connection request (ConnectToPeer).
The token must match the one from the server's ConnectToPeer message.

```
[uint32 token]
```

### Init Code 1: PeerInit

Sent to initiate a direct connection. The token is always 0 today.

**Send:**
```
[string own_username]
[string conn_type]       // "P", "F", or "D"
[uint32 token]           // always 0
```

**Receive:**
```
[string remote_username]
[string conn_type]
[uint32 token]           // always 0
```

### Modern Peer Connection Sequence

```
1. A -> Server:  ConnectToPeer(token=T, user=B, type="P")  [indirect request]
2. A -> Server:  GetPeerAddress(user=B)
3. Server -> A:  GetPeerAddress response (B's IP:port)
4. A -> B:       PeerInit(username=A, type="P", token=0)   [direct attempt]
5. Server -> B:  ConnectToPeer(user=A, ip=A_ip, port=A_port, token=T)

   If step 4 succeeds: connection established, A can send peer messages.
   If step 4 fails (firewall):
6. B -> A:       PierceFireWall(token=T)                   [reverse connect]
   If step 6 succeeds: connection established.
   If step 6 fails:
7. B -> Server:  CantConnectToPeer(token=T)
8. Server -> A:  CantConnectToPeer(token=T)
```

---

## 6. Peer Messages (Essential Subset)

### Code 9: FileSearchResponse

Sent by a peer when they have matching files for our search.

**The entire message is zlib-compressed.**

**Decompressed format:**
```
[string username]
[uint32 token]                    // matches our FileSearch token
[uint32 num_results]
iterate num_results:
  [uint8  code]                   // always 1
  [string filename]               // virtual path, e.g. "@@user\Music\Artist\track.mp3"
  [uint64 file_size]              // bytes
  [string extension]              // obsolete, often blank
  [uint32 num_attributes]
  iterate num_attributes:
    [uint32 attribute_type]       // 0=bitrate, 1=duration, 2=vbr, 4=samplerate, 5=bitdepth
    [uint32 attribute_value]
[bool   slot_free]                // uploader has a free slot
[uint32 avg_speed]                // bytes/sec upload speed
[uint32 queue_length]             // number of files in upload queue
[uint32 unknown]                  // always 0
[uint32 num_private_results]      // optional, buddy/trusted shares
iterate num_private_results:
  (same file format as above)
```

### File Attributes

| Code | Meaning | Unit |
|------|---------|------|
| 0 | Bitrate | kbps |
| 1 | Duration | seconds |
| 2 | VBR | 0=CBR, 1=VBR |
| 3 | Encoder | unused |
| 4 | Sample Rate | Hz |
| 5 | Bit Depth | bits |

Lossy files (MP3, OGG): attributes {0, 1, 2}
Lossless files (FLAC, WAV): attributes {1, 4, 5}

### Code 40: TransferRequest

Sent by the uploader when they are ready to start sending a file.

```
[uint32 direction]       // 0=download, 1=upload (always 1 in modern clients)
[uint32 token]           // unique transfer token
[string filename]        // virtual path
[uint64 file_size]       // only if direction == 1 (upload)
```

### Code 41: TransferResponse

Our response to a TransferRequest.

```
[uint32 token]
[bool   allowed]
if allowed && direction was download:
  [uint64 file_size]
if !allowed:
  [string reason]        // "Queued", "Cancelled", "File not shared.", etc.
```

### Code 43: QueueUpload

**This is how we request a file download.** We send this to tell the peer to
queue a file for upload to us.

```
[string filename]        // virtual path from search results
```

The peer will later send us a TransferRequest (code 40) when ready.

### Code 44: PlaceInQueueResponse

```
[string filename]
[uint32 place]           // position in upload queue
```

### Code 46: UploadFailed

```
[string filename]
```

### Code 50: UploadDenied

```
[string filename]
[string reason]
```

### Code 51: PlaceInQueueRequest

```
[string filename]        // ask where we are in the queue
```

---

## 7. File Messages

File messages are sent over `F` (file transfer) connections. They have NO
message code -- the connection follows a fixed sequence.

### FileTransferInit

Sent by the **uploader** after establishing the F connection:

```
[uint32 token]           // matches the TransferRequest token
```

### FileOffset

Sent by the **downloader** in response to FileTransferInit:

```
[uint64 offset]          // number of bytes already downloaded (0 for new transfer)
```

### Raw File Data

After the offset is sent, the uploader streams raw file bytes starting from
the offset position until the file is complete or the connection drops.

**There is no chunking protocol** -- the uploader just writes raw bytes to the
TCP socket. The downloader knows the expected file size from the TransferRequest
and tracks progress by counting received bytes.

---

## 8. Distributed Search Network

### How Search Works

1. Client sends `FileSearch` (server code 26) to the server with a token + query.
2. The server injects the search into the distributed tree network via
   `EmbeddedMessage` (server code 93) to branch roots.
3. Branch roots unpack the embedded `DistribSearch` (distrib code 3) and forward
   it to their children.
4. Each peer in the tree forwards the search to its own children.
5. Any peer with matching files opens a `P` connection back to the searcher
   and sends a `FileSearchResponse` (peer code 9).

### Distributed Tree Management

- After login, we send `HaveNoParent(true)` to indicate we need a parent.
- Server sends `PossibleParents` with candidate parent peers.
- We connect to candidates via `D` connections.
- Candidates send us `DistribBranchLevel` and `DistribBranchRoot`.
- The first candidate that also sends a `DistribSearch` becomes our parent.
- We send `AcceptChildren(true/false)` to control whether we accept children.

### For a Download-Only Client

**We do NOT need to participate in the distributed network.** We only need to:

1. Send `FileSearch` to the server (server code 26).
2. Listen for `FileSearchResponse` messages from peers (peer code 9).

The server will propagate our search through the distributed network on our
behalf. We just need to be reachable for peers to send us results (either by
listening on a port, or via indirect connections through the server).

However, being a good network citizen means participating in the distributed
network. For a minimal client, we can opt out by:
- Sending `HaveNoParent(true)` but not connecting to any parents.
- Sending `AcceptChildren(false)`.

---

## 9. Complete Download Flow

### Step-by-Step

```
1. LOGIN
   Client -> Server: Login(username, password, version=160, hash, minor=1)
   Server -> Client: Login response (success, banner, ip, supporter)
   Client -> Server: SetWaitPort(port)
   Client -> Server: SetStatus(2)           // online
   Client -> Server: HaveNoParent(true)
   Client -> Server: SharedFoldersFiles(0, 0)  // we share nothing

2. SEARCH
   Client -> Server: FileSearch(token=T, query="artist track name")

3. RECEIVE RESULTS (from multiple peers, over P connections)
   Peer -> Client: [P connection established]
   Peer -> Client: FileSearchResponse(token=T, results=[...])
     Each result contains: filename, size, bitrate, duration, etc.
     Plus: slot_free, avg_speed, queue_length

4. SELECT FILE & QUEUE DOWNLOAD
   Client -> Peer:  [P connection to chosen peer, if not already established]
   Client -> Peer:  QueueUpload(filename)    // "please upload this file to me"

5. WAIT FOR TRANSFER REQUEST
   Peer -> Client:  TransferRequest(direction=1, token=T2, filename, filesize)
   Client -> Peer:  TransferResponse(token=T2, allowed=true)

6. FILE TRANSFER (over F connection)
   Peer -> Client:  [F connection established]
   Peer -> Client:  FileTransferInit(token=T2)
   Client -> Peer:  FileOffset(offset=0)     // start from beginning
   Peer -> Client:  [raw file bytes until complete]

7. COMPLETION
   Connection closes. File is complete when received_bytes == filesize.
```

### Resuming Downloads

To resume a partial download, send `FileOffset(offset=bytes_already_received)`.
The uploader starts sending from that offset.

### Queue System

- Soulseek uses a **queue-based** upload system.
- When we send `QueueUpload`, the peer adds us to their queue.
- The peer decides when to start the transfer (first-come-first-served, with
  priority for supporters/friends).
- We can check our position with `PlaceInQueueRequest` / `PlaceInQueueResponse`.
- Typical wait times: seconds to minutes for popular files, longer for users
  with many queued uploads.

### No Byte-Range Requests

**The Soulseek protocol does NOT support byte-range requests.** The only
seeking mechanism is `FileOffset`, which is designed for resuming interrupted
downloads, not for random seeking. Once a transfer starts, data flows
sequentially from the offset to the end of the file.

This means: **we cannot seek within a file during streaming.** The streaming
strategy must work with sequential data delivery.

---

## 10. Streaming Strategy for NX Player

### Architecture

```
+----------+     search      +-----------+    TCP     +-----------+
| Frontend | <-------------> | Soulseek  | <--------> | Soulseek  |
| (Svelte) |   IPC/Tauri     | Engine    |            | Network   |
|          |                 | (Rust)    |            | (Server + |
|          |   HTTP Range    |           |            |  Peers)   |
|          | <-------------> | HTTP      |            +-----------+
| <audio>  |   localhost     | Server    |
+----------+                 +-----------+
                                  |
                             +----v----+
                             | Temp    |
                             | File    |
                             | (disk)  |
                             +---------+
```

### Sequential Streaming Approach

Since Soulseek only supports sequential transfer (no byte ranges), the
streaming strategy is:

1. **Start download**: Write incoming bytes to a temp file on disk.
2. **Buffer threshold**: Wait for first ~500KB (or ~10 seconds of audio at
   128kbps) before signaling playback readiness.
3. **Local HTTP server**: Serve the temp file via `localhost:PORT` with
   HTTP Range request support. The `<audio>` element requests ranges as needed.
4. **Progressive availability**: The HTTP server knows how many bytes have been
   downloaded. For Range requests beyond the downloaded portion, either:
   - Return 416 (Range Not Satisfiable) and let the frontend retry
   - Block/wait until the data is available (simpler but risks timeout)
   - Return a partial response with what's available
5. **Seek limitation**: Seeking backward works (data already on disk). Seeking
   forward only works up to the downloaded boundary. Seeking beyond requires
   waiting for the download to catch up.

### Buffer Size Calculations

| Bitrate | 10 sec buffer | 30 sec buffer |
|---------|---------------|---------------|
| 128 kbps | 160 KB | 480 KB |
| 192 kbps | 240 KB | 720 KB |
| 256 kbps | 320 KB | 960 KB |
| 320 kbps | 400 KB | 1.2 MB |
| FLAC ~1000 kbps | 1.25 MB | 3.75 MB |

**Recommendation**: Use a **500 KB initial buffer** for lossy formats, **2 MB
for lossless**. This gives ~15-30 seconds of head start at typical bitrates.

### Transfer Speed Expectations

Soulseek speeds depend on the uploader's bandwidth and queue position:
- **Best case**: 1-10 MB/s (fast uploader, free slot)
- **Typical**: 100 KB/s - 1 MB/s
- **Worst case**: 10-100 KB/s (slow uploader or congested)

At 128 kbps MP3, you need only ~16 KB/s sustained to keep up with playback.
Most transfers will be faster than this, making streaming viable for lossy
formats. Lossless FLAC at 1000 kbps needs ~125 KB/s -- usually achievable but
less margin.

### Handling Multiple Sources

For reliability, we could:
1. Search returns multiple peers with the same file.
2. If primary download is too slow or stalls, fall back to another peer.
3. **Cannot download from multiple peers simultaneously** for the same file
   (the protocol doesn't support chunked multi-source downloads like
   BitTorrent).

### Pre-Caching Strategy

For album playback:
1. Start downloading the current track.
2. Once current track is 50%+ downloaded, start queueing the next track.
3. By the time the current track finishes, the next track may already have
   enough data buffered for instant playback.

---

## 11. Rust Implementation Architecture

### Recommended Crates

```toml
[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Byte manipulation
bytes = "1"
byteorder = "1"          # little-endian integer read/write

# Compression
flate2 = "1"             # zlib decompress for search results

# Hashing
md5 = "0.7"              # MD5 for login hash

# HTTP server (for streaming)
hyper = { version = "1", features = ["http1", "server"] }
http-body-util = "0.1"

# Serialization (for IPC with frontend)
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

### Module Structure

```
src-tauri/src/soulseek/
    mod.rs                  // re-exports
    protocol/
        mod.rs
        types.rs            // primitive pack/unpack (uint32, string, ip, etc.)
        server_messages.rs  // Login, FileSearch, ConnectToPeer, etc.
        peer_messages.rs    // FileSearchResponse, TransferRequest, QueueUpload, etc.
        file_messages.rs    // FileTransferInit, FileOffset
        distributed.rs      // DistribSearch, BranchLevel, BranchRoot (minimal)
        codes.rs            // message code constants
    connection/
        mod.rs
        server.rs           // ServerConnection: connect, login, send/recv loop
        peer.rs             // PeerConnection: init handshake, message exchange
        file_transfer.rs    // FileConnection: transfer init, offset, raw data
        manager.rs          // ConnectionManager: track all connections, reconnect
    search/
        mod.rs
        engine.rs           // send search, collect results, rank/deduplicate
        result.rs           // SearchResult struct, ranking logic
    transfer/
        mod.rs
        download.rs         // DownloadManager: queue, progress, write to disk
        queue.rs            // queue position tracking, retry logic
    stream/
        mod.rs
        http_server.rs      // local HTTP server with Range support
        buffer.rs           // progressive buffer tracking
    client.rs               // high-level SoulseekClient orchestrating everything
    config.rs               // credentials, server address, ports
```

### Key Design Decisions

**1. Async with Tokio**

Every connection runs as a Tokio task. The server connection is a long-lived
task that reads/writes from a channel. Peer and file connections are spawned
as needed.

```rust
// Conceptual structure
struct SoulseekClient {
    server_tx: mpsc::Sender<ServerMessage>,    // send to server connection task
    event_rx: broadcast::Receiver<SoulseekEvent>, // receive events from all connections
    connection_manager: ConnectionManager,
    download_manager: DownloadManager,
}
```

**2. Message Pack/Unpack**

```rust
// types.rs - core primitives
pub fn pack_uint32(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

pub fn pack_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    pack_uint32(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
}

pub fn unpack_uint32(data: &[u8], offset: usize) -> (usize, u32) {
    let val = u32::from_le_bytes(data[offset..offset+4].try_into().unwrap());
    (offset + 4, val)
}

pub fn unpack_string(data: &[u8], offset: usize) -> (usize, String) {
    let (next, len) = unpack_uint32(data, offset);
    let s = String::from_utf8_lossy(&data[next..next + len as usize]).into_owned();
    (next + len as usize, s)
}
```

**3. Server Connection Loop**

```rust
// Pseudocode for server read loop
async fn server_read_loop(stream: TcpStream, event_tx: Sender<Event>) {
    let mut buf = BytesMut::new();
    loop {
        stream.read_buf(&mut buf).await?;
        while buf.len() >= 8 {
            let msg_len = u32::from_le_bytes(buf[0..4]) as usize;
            let total = msg_len + 4;
            if buf.len() < total { break; }

            let msg_code = u32::from_le_bytes(buf[4..8]);
            let payload = &buf[8..total];
            let event = parse_server_message(msg_code, payload);
            event_tx.send(event).await?;
            buf.advance(total);
        }
    }
}
```

**4. File Transfer with Streaming**

```rust
// Pseudocode for file download with progressive write
async fn download_file(
    stream: TcpStream,
    token: u32,
    offset: u64,
    file_size: u64,
    temp_path: PathBuf,
    progress_tx: watch::Sender<u64>,  // bytes downloaded so far
) {
    // 1. Read FileTransferInit
    let recv_token = read_u32(&stream).await?;
    assert_eq!(recv_token, token);

    // 2. Send FileOffset
    write_u64(&stream, offset).await?;

    // 3. Stream raw bytes to temp file
    let mut file = OpenOptions::new().create(true).append(true).open(&temp_path)?;
    let mut received = offset;
    let mut buf = vec![0u8; 65536];

    while received < file_size {
        let n = stream.read(&mut buf).await?;
        if n == 0 { break; } // connection closed
        file.write_all(&buf[..n])?;
        received += n as u64;
        progress_tx.send(received)?;
    }
}
```

**5. Local HTTP Server for Audio Streaming**

```rust
// Serve the partially-downloaded file with Range support
async fn handle_request(
    req: Request<Body>,
    temp_path: PathBuf,
    downloaded: watch::Receiver<u64>,  // how many bytes are available
) -> Response<Body> {
    let available = *downloaded.borrow();
    let file = File::open(&temp_path)?;

    if let Some(range) = req.headers().get("Range") {
        let (start, end) = parse_range(range, available);
        if start >= available {
            return Response::builder().status(416).body(Body::empty());
        }
        let end = end.min(available - 1);
        let len = end - start + 1;

        file.seek(SeekFrom::Start(start))?;
        let body = read_exact(&file, len);

        Response::builder()
            .status(206)
            .header("Content-Range", format!("bytes {start}-{end}/{available}"))
            .header("Content-Length", len)
            .header("Accept-Ranges", "bytes")
            .body(body)
    } else {
        // Full response with what's available
        // ...
    }
}
```

### Search Result Ranking

For music streaming, rank results by:

1. **Has free upload slot** (immediate transfer vs. queued)
2. **Audio quality** (prefer 320kbps or FLAC over 128kbps)
3. **Upload speed** (faster = better streaming experience)
4. **Queue length** (shorter queue = sooner transfer)
5. **File size sanity** (filter out obviously wrong sizes)
6. **Filename quality** (prefer clean names, correct extensions)

```rust
struct RankedResult {
    username: String,
    filename: String,
    file_size: u64,
    bitrate: Option<u32>,
    duration: Option<u32>,
    sample_rate: Option<u32>,
    bit_depth: Option<u32>,
    is_vbr: bool,
    slot_free: bool,
    avg_speed: u32,
    queue_length: u32,
    score: f64,        // computed ranking score
}
```

### Integration with Existing Tauri App

The Soulseek engine runs as a background Tokio runtime within the Tauri app.
Expose Tauri commands:

```rust
#[tauri::command]
async fn slsk_login(username: String, password: String) -> Result<(), String>;

#[tauri::command]
async fn slsk_search(query: String) -> Result<Vec<SearchResult>, String>;

#[tauri::command]
async fn slsk_download(username: String, filename: String, size: u64)
    -> Result<StreamInfo, String>;
// StreamInfo contains the local HTTP URL for the audio element

#[tauri::command]
async fn slsk_download_progress(token: u32) -> Result<DownloadProgress, String>;

#[tauri::command]
async fn slsk_cancel_download(token: u32) -> Result<(), String>;
```

---

## 12. Legal Considerations

### Soulseek Terms of Service

The Soulseek network is maintained by Nir Arbel. There is no published API or
developer terms of service. The protocol is proprietary but has been
reverse-engineered by multiple projects over 20+ years.

### Third-Party Client Precedent

Multiple third-party clients exist and operate openly:

| Client | Language | License | Status |
|--------|----------|---------|--------|
| **Nicotine+** | Python | GPL-3.0 | Active, most complete |
| **slskd** | C# (.NET) | AGPL-3.0 | Active, web-based |
| **Museek+** | C++ | GPL-2.0 | Unmaintained |
| **Soulseek.NET** | C# | GPL-3.0 | Active library used by slskd |
| **Seeker** | Android/Java | GPL-3.0 | Active |
| **soulseeX** | macOS/ObjC | GPL-2.0 | Active |

All third-party clients operate without explicit permission but are tolerated.
The Nicotine+ project has been active for 20+ years. The Soulseek admin (Nir)
has participated in discussions with the Nicotine+ developers regarding
protocol behavior.

### Protocol Extension Warning

From the Nicotine+ protocol documentation:

> "The protocol is old and rigid, with various client implementations existing
> in the wild. Careful coordination between clients is necessary. Please don't
> extend the protocol without the approval of Soulseek's administrators."

### Recommendations

1. **Do not extend the protocol.** Only implement existing message types.
2. **Be a good citizen.** Share files if possible, don't hammer the server.
3. **Identify your client.** Use a unique version number in the Login message
   so Soulseek admins can identify your client if there are issues.
4. **Use the GPL.** All existing third-party implementations use GPL-compatible
   licenses. If you base your implementation on their protocol documentation
   (which we are), using GPL or a compatible license is prudent.
5. **Rate-limit searches.** Don't flood the network with searches.
6. **Support the community.** Consider donating to Soulseek
   (https://www.slsknet.org/donate) and encouraging users to do the same.

---

## Appendix: Quick Reference

### Minimum Viable Feature Set for Streaming

1. **Login** (server code 1 + 2 + 28 + 35)
2. **Search** (server code 26)
3. **Receive search results** (peer code 9, handle incoming P connections)
4. **Request download** (peer code 43 - QueueUpload)
5. **Accept transfer** (peer code 40/41 - TransferRequest/Response)
6. **Receive file** (F connection - FileTransferInit + FileOffset + raw bytes)
7. **Serve via HTTP** (local server for `<audio>` element)

### Messages NOT Needed for Streaming

- Chat rooms (codes 13-17)
- Private messages (code 22)
- Interests/recommendations (codes 51-57, 110-112)
- Room management (codes 113-152)
- Distributed network participation (can opt out)
- File sharing / uploading (we are download-only)
- User info/browse (codes 15-16, 4-5)

### Port Requirements

- **Outbound TCP**: Port 2242 (server), any port (peers) -- always works
- **Inbound TCP**: Ideally one open port (default 2234) for direct peer
  connections. Without it, we rely entirely on indirect connections via the
  server, which is slower but functional.
