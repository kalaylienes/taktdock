# Recordings

The meow is the only sound in TaktDock that is not synthesised. Both files are
real kittens, released into the public domain under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/). CC0 asks for no
attribution; they are credited here anyway.

| File | What | Source | License |
| --- | --- | --- | --- |
| `kitten-3-weeks.wav` | A three week old kitten. Plays on the beat, and quieter on the clicks between beats. | [barkenov, "Kitten miaowing.wav"](https://freesound.org/people/barkenov/sounds/440697/) | CC0 1.0 |
| `kitten-8-weeks.wav` | An eight week old kitten. Plays on the downbeat. | [Luke100000, "Kitten meows"](https://freesound.org/people/Luke100000/sounds/476918/) | CC0 1.0 |

How they were cut from the originals, so it can be done again:

- One meow from each: 6.582 to 6.970 s of the first recording, 29.891 to
  30.450 s of the second.
- The start of each is placed on the kitten's first audible breath rather than
  on silence before it, because the beat is heard where the meow begins.
- High pass at 180 Hz for room rumble, a 3 ms fade in, and a fade out over the
  last 70 to 80 ms.
- Mono, 48 kHz, 16 bit PCM. The app resamples to whatever rate the output
  device runs at.

```
ffmpeg -i kitten3w.wav -af "atrim=start=6.582:end=6.97,asetpts=N/SR/TB,highpass=f=180,afade=t=in:d=0.003,afade=t=out:st=0.318:d=0.07" -ac 1 -ar 48000 -c:a pcm_s16le kitten-3-weeks.wav
ffmpeg -i kitten8w.wav -af "atrim=start=29.891:end=30.45,asetpts=N/SR/TB,highpass=f=180,afade=t=in:d=0.003,afade=t=out:st=0.479:d=0.08" -ac 1 -ar 48000 -c:a pcm_s16le kitten-8-weeks.wav
```
