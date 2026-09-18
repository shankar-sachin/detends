# Glass

Glass is a material, not decoration (rule 9). A pane in détends is a physical
object: it has a thickness, a rounded bevel at its edge, and an index of
refraction. Everything the material does follows from those three numbers.

## Why it is not just a blur

The defining property of the détends slab is that **refraction is zero in the
flat interior by construction** — not by a mask that fades it out, but because a
flat surface has a vertical normal and a vertical normal bends nothing. Content
under the middle of a pane stays clean and readable; only the rim bends what is
behind it.

An earlier design displaced the backdrop along the SDF gradient. That does not
work: `∇d` is a unit vector whose *direction is constant* along the normal from
any edge point, so it produces a uniform outward smear — a vignette, not glass.

## The fragment shader, in order

1. **Superellipse SDF.** Exponent 4–6 gives continuous curvature, not a
   circular arc. Rule 17 asks for few rounded rectangles, so the ones that exist
   are optically correct.
2. **Analytic gradient.** Exact rather than finite-differenced; an estimated
   normal shimmers as the pane moves, which is exactly the low-level noise that
   makes an interface feel unsettled.
3. **Height field**, derived from the SDF rather than sampled from a texture:
   `t = clamp(-d/bevel, 0, 1)`, `h = thickness · √(1-(1-t)²)`. A rounded rim
   rising to full thickness, and flat beyond it.
4. **3D normal** from `dh/dp`. In the interior `dh/dt = 0`, so the normal is
   exactly `(0,0,1)` — the property that makes step 5 vanish there.
5. **Refraction.** `refract(I, N, 1/ior)` with `ior ≈ 1.48`, then slab parallax:
   the ray crosses `h` of glass and exits displaced by `h·tan θ`. Clamped, or
   grazing rays smear across the screen.
6. **Dispersion.** Three taps at slightly different displacements. Because the
   offset is already zero in the interior, the colour split appears only at the
   rim without needing a mask — the physics does the masking.
7. **Frost.** Mixes the sharp backdrop with the blur pyramid, so a pane goes
   from nearly clear to deeply diffused with one number.
8. **Fresnel.** `0.04 + 0.96·(1 − N·−I)⁵`, driving the rim specular. This does
   more to make a pane feel physical than the dispersion does; without it the
   surface reads as a blurred hole.
9. **Grain.** A whisper of blue noise. A wide blur over a smooth gradient is the
   most band-prone thing a compositor draws.
10. **Coverage AA** from `fwidth` on the exact SDF. This is why the pipeline
    needs no MSAA anywhere.

## The passes

```
A  environment   → env            Rgba16Float, full res
B  blur pyramid  → blurred        dual-Kawase, 3 down + 2 up, half res
C₀ layer 0       → composite[0]   blit env, then its glass and fills
B′ re-blur       → blurred        layer 1 refracts layer 0's result
C₁ layer 1       → composite[1]
B″ re-blur       → blurred
C₂ layer 2       → composite[0]   ping-ponging back
E  present       → surface        sRGB encode, then dither
```

**Layers exist because one pass cannot composite overlapping glass.** Every pane
would sample the same untouched backdrop, so a panel over another panel would
refract the wallpaper rather than the panel beneath it, and the lower pane's rim
would shine straight through. Surfaces within a layer must not overlap, which is
checked in debug builds; that constraint keeps each layer a single instanced
draw.

Instances for *every* layer are gathered and uploaded **before any pass is
recorded**, because `Queue::write_buffer` does not interleave with command
encoding — all writes land before the command buffer runs. Uploading per layer
leaves every pass reading whichever layer wrote last.

## Colour

Everything is linear until the final pass. The environment, the pyramid, the
glass and the text all live in `Rgba16Float`.

The present pass encodes to sRGB **and then dithers**, in that order, into a
*non-sRGB* `Bgra8Unorm` surface. A hardware `*Srgb` format would encode after
the shader runs, compressing the dither non-uniformly and leaving visible
banding in the darks — precisely where a calm, near-black environment lives.

The dither is TPDF blue noise at one 8-bit step, and it is **static, not
animated per frame**: temporal dither on a motionless screen shimmers, which is
the opposite of calm.

On a display that offers `ExtendedSrgbLinear`, the encode and dither are skipped
and linear light goes straight out — three lines, decided by one enum.

## The blur

Dual-Kawase: 5-tap downsample, 8-tap upsample, offsets derived from the
**source** texture's size in both directions (deriving them from the destination
is the classic bug, and makes the result look boxy). Linear filtering is
mandatory — the half-texel offsets exist so each tap becomes a 2×2 average, and
a nearest sampler silently gives a completely different kernel.

Three levels rather than four. Four reaches a wider radius but the blur starts
to swim when content moves behind it; radius is tuned with the offset scale
instead, which stays temporally stable.
