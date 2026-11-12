import asyncio
import os
import sys

sys.path.insert(0, sys.argv[1])
import h2.config
import h2.connection
import h2.events

port_file = sys.argv[2]
cert = sys.argv[3] if len(sys.argv) > 3 else None
import ssl


class Handler(asyncio.Protocol):
    def connection_made(self, transport):
        self.transport = transport
        self.conn = h2.connection.H2Connection(config=h2.config.H2Configuration(client_side=False))
        self.conn.initiate_connection()
        self.transport.write(self.conn.data_to_send())
        self.streams = {}
        self.pending = {}

    def data_received(self, data):
        for ev in self.conn.receive_data(data):
            if isinstance(ev, h2.events.RequestReceived):
                self.streams[ev.stream_id] = {"headers": dict(ev.headers), "body": b""}
            elif isinstance(ev, h2.events.DataReceived):
                self.streams[ev.stream_id]["body"] += ev.data
                self.conn.acknowledge_received_data(ev.flow_controlled_length, ev.stream_id)
            elif isinstance(ev, h2.events.StreamEnded):
                self.respond(ev.stream_id)
            elif isinstance(ev, h2.events.WindowUpdated):
                self.flush_pending()
            elif isinstance(ev, h2.events.PingReceived):
                pass
        self.transport.write(self.conn.data_to_send())

    def respond(self, sid):
        st = self.streams[sid]
        path = st["headers"][b":path"].decode()
        extra = []
        body = b"hello"
        if path == "/echo":
            body = st["body"]
        elif path == "/big":
            body = bytes((i * 7) & 255 for i in range(3 * 1024 * 1024))
        elif path == "/headers":
            extra = [("x-h%d" % i, "".join(chr(97 + (i * 31 + j * 17) % 26) for j in range(600))) for i in range(60)]
        elif path == "/ua":
            body = st["headers"].get(b"x-test", b"none")
        elif path == "/missing":
            self.conn.send_headers(sid, [(":status", "404")], end_stream=True)
            return
        self.conn.send_headers(sid, [(":status", "200"), ("content-length", str(len(body)))] + extra)
        self.pending[sid] = body
        self.flush_pending()

    def flush_pending(self):
        for sid in list(self.pending):
            body = self.pending[sid]
            while body:
                window = self.conn.local_flow_control_window(sid)
                n = min(window, len(body), self.conn.max_outbound_frame_size)
                if n <= 0:
                    break
                self.conn.send_data(sid, body[:n])
                body = body[n:]
            if body:
                self.pending[sid] = body
            else:
                self.conn.end_stream(sid)
                del self.pending[sid]
        self.transport.write(self.conn.data_to_send())


async def main():
    loop = asyncio.get_running_loop()
    ctx = None
    if cert:
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(cert, cert + ".key")
        ctx.set_alpn_protocols(["h2"])
    server = await loop.create_server(Handler, "127.0.0.1", 0, ssl=ctx)
    port = server.sockets[0].getsockname()[1]
    with open(port_file, "w") as f:
        f.write(str(port))
    async with server:
        await server.serve_forever()


asyncio.run(main())
