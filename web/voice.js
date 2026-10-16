// Push-to-talk (conker_client::voice): the client calls window.conkerVoice's talk(on) when the
// PushToTalk key (V) goes down or up, play(slot, packet) for each packet the server relays, and
// mute(on) on M. The first talk asks for the microphone; then while the key is held, an AudioWorklet
// takes its samples and WebCodecs encodes them to Opus (mono, 24 kbps, 20 ms packets), each packet
// to the client's voice_frame. Each speaker has its own decoder; its packets play in order on one
// AudioContext, AHEAD seconds late so a late packet still plays in time.
// Needs WebCodecs' audio (Chrome 94, Firefox 130, Safari 26): without it, nothing is sent or played.

/** Seconds of buffer a speaker's playback starts with, and the most it may drift ahead. */
const AHEAD = 0.08;
const BEHIND = 0.5;

// Buffers the microphone 960 samples at a time (20 ms at 48 kHz) for the encoder.
const WORKLET = URL.createObjectURL(
  new Blob(
    [
      `registerProcessor("mic", class extends AudioWorkletProcessor {
  buf = new Float32Array(960);
  n = 0;
  process([input]) {
    for (const s of input[0] ?? []) {
      this.buf[this.n++] = s;
      if (this.n === this.buf.length) {
        this.port.postMessage(this.buf);
        this.buf = new Float32Array(960);
        this.n = 0;
      }
    }
    return true;
  }
});`,
    ],
    { type: "text/javascript" },
  ),
);

/** Sets window.conkerVoice; `send` takes each encoded packet (the client's voice_frame). */
export function voice(send) {
  const line = document.getElementById("voice");
  const note = (text) => line && (line.textContent = text);
  let ctx, master;
  let muted = false;
  let talking = false;
  let mic = null; // a Promise of the encoder, from the first talk (null again if it failed)
  const speakers = new Map();

  const audio = () => {
    if (!ctx) {
      ctx = new AudioContext();
      master = ctx.createGain();
      master.gain.value = muted ? 0 : 1;
      master.connect(ctx.destination);
    }
    if (ctx.state !== "running") ctx.resume().catch(() => {});
    return ctx;
  };

  const open = async () => {
    if (typeof AudioEncoder === "undefined") throw new Error("voice chat needs a newer browser");
    const c = audio();
    const config = { codec: "opus", sampleRate: c.sampleRate, numberOfChannels: 1, bitrate: 24000 };
    if (!(await AudioEncoder.isConfigSupported(config)).supported)
      throw new Error("this browser can't encode voice (Opus)");
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
    });
    const encoder = new AudioEncoder({
      output: (chunk) => {
        const packet = new Uint8Array(chunk.byteLength);
        chunk.copyTo(packet);
        send(packet);
      },
      error: (e) => console.warn("voice: encoder:", e),
    });
    encoder.configure(config);
    await c.audioWorklet.addModule(WORKLET);
    // Its output is silence; connected so every browser runs it.
    const node = new AudioWorkletNode(c, "mic");
    node.connect(c.destination);
    let timestamp = 0;
    node.port.onmessage = ({ data }) => {
      if (!talking || encoder.state !== "configured") return;
      const frames = data.length;
      encoder.encode(
        new AudioData({ format: "f32-planar", sampleRate: c.sampleRate, numberOfFrames: frames, numberOfChannels: 1, timestamp, data }),
      );
      timestamp += Math.round((frames / c.sampleRate) * 1e6);
    };
    c.createMediaStreamSource(stream).connect(node);
    return encoder;
  };

  // A speaker's decoded packet, after the last one; a new start (or a stall) AHEAD from now. None
  // while the page has had no click or key yet (the context waits for one: they'd pile up).
  const schedule = (s, data) => {
    if (ctx.state !== "running") return data.close();
    const buffer = ctx.createBuffer(1, data.numberOfFrames, data.sampleRate);
    data.copyTo(buffer.getChannelData(0), { planeIndex: 0, format: "f32-planar" });
    data.close();
    const now = ctx.currentTime;
    if (s.at < now || s.at > now + BEHIND) s.at = now + AHEAD;
    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(master);
    source.start(s.at);
    s.at += buffer.duration;
  };

  window.conkerVoice = {
    talk(on) {
      talking = on;
      if (!on) return void mic?.then((e) => e?.flush().catch(() => {}));
      mic ??= open().then(
        (encoder) => (note(""), encoder),
        (e) => {
          mic = null;
          const blocked = e?.name === "NotAllowedError";
          note(blocked ? "Voice: the microphone is blocked (allow it in the address bar)" : `Voice: ${e?.message ?? e}`);
          console.warn("voice:", e);
          return null;
        },
      );
    },
    play(slot, packet) {
      if (typeof AudioDecoder === "undefined") return;
      audio();
      let s = speakers.get(slot);
      if (!s) {
        s = { at: 0, t: 0 };
        s.decoder = new AudioDecoder({
          output: (data) => schedule(s, data),
          error: (e) => (console.warn(`voice: slot ${slot}:`, e), speakers.delete(slot)),
        });
        s.decoder.configure({ codec: "opus", sampleRate: 48000, numberOfChannels: 1 });
        speakers.set(slot, s);
      }
      s.decoder.decode(new EncodedAudioChunk({ type: "key", timestamp: (s.t += 20000), data: packet }));
    },
    mute(on) {
      muted = on;
      if (master) master.gain.value = on ? 0 : 1;
    },
  };
}
