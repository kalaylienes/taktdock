# Recordings

The meow and the bark are the two sounds in TaktDock that are not
synthesised. All four files are real animals, released into the public domain
under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/). CC0 asks for no
attribution; they are credited here anyway.

| File | What | Source | License |
| --- | --- | --- | --- |
| `kitten-3-weeks.wav` | A three week old kitten. Plays on the beat, and quieter on the clicks between beats. | [barkenov, "Kitten miaowing.wav"](https://freesound.org/people/barkenov/sounds/440697/) | CC0 1.0 |
| `kitten-8-weeks.wav` | An eight week old kitten. Plays on the downbeat. | [Luke100000, "Kitten meows"](https://freesound.org/people/Luke100000/sounds/476918/) | CC0 1.0 |
| `puppy-maltipoo.wav` | A Maltipoo puppy asking for something. Plays on the beat, and quieter on the clicks between beats. | [YUXUANZHAO, "Bark 1.wav"](https://freesound.org/people/YUXUANZHAO/sounds/625274/) | CC0 1.0 |
| `puppy-cockapoo.wav` | A Cockapoo puppy. Plays on the downbeat. | [dtmendes, "Dog barking and making noises.wav"](https://freesound.org/people/dtmendes/sounds/591137/) | CC0 1.0 |

How they were cut from the originals, so it can be done again:

- One sound from each: 6.582 to 6.970 s of the three week kitten, 29.891 to
  30.450 s of the eight week kitten, 0.219 to 0.475 s of the Maltipoo and
  2.577 to 2.830 s of the Cockapoo.
- The start of each is placed on the animal's first audible breath rather than
  on silence before it, because the beat is heard where the sound begins.
- High pass at 180 Hz (150 Hz for the dogs) for room rumble, a 3 ms fade in,
  and a fade out over the last 40 to 80 ms. The Cockapoo is taken down 10%
  first, because the preview it was cut from touches full scale.
- The source files are the freesound.org high quality previews, decoded to
  48 kHz mono first.
- Mono, 48 kHz, 16 bit PCM. The app resamples to whatever rate the output
  device runs at.

```
ffmpeg -i kitten3w.wav -af "atrim=start=6.582:end=6.97,asetpts=N/SR/TB,highpass=f=180,afade=t=in:d=0.003,afade=t=out:st=0.318:d=0.07" -ac 1 -ar 48000 -c:a pcm_s16le kitten-3-weeks.wav
ffmpeg -i kitten8w.wav -af "atrim=start=29.891:end=30.45,asetpts=N/SR/TB,highpass=f=180,afade=t=in:d=0.003,afade=t=out:st=0.479:d=0.08" -ac 1 -ar 48000 -c:a pcm_s16le kitten-8-weeks.wav
ffmpeg -i maltipoo.wav -af "atrim=start=0.219:end=0.475,asetpts=N/SR/TB,highpass=f=150,afade=t=in:d=0.003,afade=t=out:st=0.216:d=0.04" -ac 1 -ar 48000 -c:a pcm_s16le puppy-maltipoo.wav
ffmpeg -i cockapoo.wav -af "atrim=start=2.577:end=2.83,asetpts=N/SR/TB,volume=0.9,highpass=f=150,afade=t=in:d=0.003,afade=t=out:st=0.213:d=0.04" -ac 1 -ar 48000 -c:a pcm_s16le puppy-cockapoo.wav
```
