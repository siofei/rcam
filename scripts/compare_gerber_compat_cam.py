"""Read-only local gerbv image comparison for three compatibility samples.

Run after `real_compatibility_cam_artifacts`. Source Gerbers and rendered PNGs
remain in the ignored local evidence directory; the JSON report is metrics only.
"""
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "evidence/gerber-compat-final-20260924/cam"
SAMPLES = {
    "ep11-top": (
        "tests/GERBER/13/EP11BAM-A_top_0mm_202607241511.gbr",
        (-364.09003, -281.47501, 476.91997, 312.53499),
    ),
    "ep11-bottom": (
        "tests/GERBER/13/EP11BAM-A_bot_0mm_202607241511.gbr",
        (-364.09003, -281.47501, 476.91997, 312.53499),
    ),
    "art08": (
        "0727SMT/75/ea1hs2m01MAN_VB/art08.art",
        (-25.2751, -177.2254, 77.7028, 26.4852),
    ),
}


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(*args):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"{args!r}: {result.returncode}: {result.stderr}")
    return result.stdout.strip()


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    report = {
        "gerbv_version": run("gerbv", "-V"),
        "dpi": 160,
        "samples": {},
    }
    for name, (relative, bounds) in SAMPLES.items():
        source = ROOT / relative
        normalized = OUT / f"{name}-normalized.gbr"
        source_sha = sha(source)
        lower_x, lower_y, upper_x, upper_y = bounds
        origin = (lower_x - 5, lower_y - 5)
        window = (upper_x - lower_x + 10, upper_y - lower_y + 10)
        origin_arg = f"{origin[0] / 25.4:.3f}x{origin[1] / 25.4:.3f}"
        window_arg = f"{window[0] / 25.4:.3f}x{window[1] / 25.4:.3f}"
        images = []
        for label, path in (("source", source), ("normalized", normalized)):
            image = OUT / f"{name}-{label}-cam.png"
            run("gerbv", "-x", "png", "-D", "160", "-W", window_arg,
                f"-O={origin_arg}", "-o", str(image), str(path))
            images.append(image)
        diff = subprocess.run(
            ["magick", "compare", "-metric", "AE", str(images[0]), str(images[1]), "null:"],
            capture_output=True, text=True,
        )
        if diff.returncode not in (0, 1):
            raise RuntimeError(diff.stderr)
        dimensions = run("magick", "identify", "-format", "%wx%h", str(images[0]))
        mean = float(run("magick", str(images[0]), "-format", "%[fx:mean]", "info:"))
        if mean <= 0:
            raise RuntimeError(f"{name}: independent renderer produced an empty image")
        report["samples"][name] = {
            "source_path": relative,
            "source_sha256_before_after": [source_sha, sha(source)],
            "normalized_sha256": sha(normalized),
            "image_size": dimensions,
            "fixed_bounds_mm": bounds,
            "origin_inch": origin_arg,
            "window_inch": window_arg,
            "different_pixels": int(diff.stderr.split()[0]),
            "source_mean_intensity": mean,
            "source_image_sha256": sha(images[0]),
            "normalized_image_sha256": sha(images[1]),
        }
        print(name, report["samples"][name]["different_pixels"], dimensions)
    destination = OUT.parent / "independent-cam-comparison.json"
    destination.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(destination)


if __name__ == "__main__":
    main()
