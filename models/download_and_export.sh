#!/usr/bin/env bash
# Downloads Microsoft OmniParser-v2.0 `icon_detect/model.pt` from HuggingFace and
# exports it to ONNX in multiple flavors:
#
#   icon_detect.onnx           fp32, imgsz=1280  (baseline)
#   icon_detect_640.onnx       fp32, imgsz=640
#   icon_detect_int8.onnx      int8 dynamic quant of the 1280 fp32 model
#   icon_detect_640_int8.onnx  int8 dynamic quant of the 640  fp32 model
#
# Idempotent per-output: skips work when the target already exists.
# Creates a throwaway venv at /tmp/yolo-export to isolate the python deps.
set -euo pipefail

MODELS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_1280_FP32="${MODELS_DIR}/icon_detect.onnx"
OUT_640_FP32="${MODELS_DIR}/icon_detect_640.onnx"
OUT_1280_INT8="${MODELS_DIR}/icon_detect_int8.onnx"
OUT_640_INT8="${MODELS_DIR}/icon_detect_640_int8.onnx"
VENV_DIR="/tmp/yolo-export"
PY="${VENV_DIR}/bin/python"

need_any=0
for f in "${OUT_1280_FP32}" "${OUT_640_FP32}" "${OUT_1280_INT8}" "${OUT_640_INT8}"; do
    if [[ ! -f "$f" ]]; then
        need_any=1
    fi
done
if [[ "${need_any}" -eq 0 ]]; then
    echo "all four model variants already exist under ${MODELS_DIR}; nothing to do"
    exit 0
fi

# Prefer a system python3 that isn't Python 3.14 (ultralytics wheels may not
# exist for 3.14 yet). Fall back gracefully.
PYTHON_BIN=""
for cand in python3.12 python3.11 python3.10 python3.13 python3; do
    if command -v "$cand" >/dev/null 2>&1; then
        VER="$($cand -c 'import sys; print("%d.%d"%sys.version_info[:2])' 2>/dev/null || echo)"
        case "$VER" in
            3.10|3.11|3.12|3.13) PYTHON_BIN="$cand"; break ;;
        esac
    fi
done
if [[ -z "$PYTHON_BIN" ]]; then
    echo "No suitable python3 (3.10-3.13) found; falling back to python3" >&2
    PYTHON_BIN="python3"
fi
echo "using ${PYTHON_BIN} ($(${PYTHON_BIN} -V))"

if [[ ! -d "${VENV_DIR}" ]]; then
    "${PYTHON_BIN}" -m venv "${VENV_DIR}"
fi

# Force public PyPI — the base environment here points at a private AWS
# CodeArtifact mirror that doesn't carry these wheels.
PIP_EXTRA=(--index-url "https://pypi.org/simple" --no-cache-dir)
"${PY}" -m pip install --quiet "${PIP_EXTRA[@]}" --upgrade pip
"${PY}" -m pip install --quiet "${PIP_EXTRA[@]}" huggingface_hub ultralytics onnx onnxsim onnxruntime

DOWNLOADED_PT="$(
    "${PY}" - <<'PY'
from huggingface_hub import hf_hub_download
p = hf_hub_download(repo_id="microsoft/OmniParser-v2.0", filename="icon_detect/model.pt")
print(p)
PY
)"
echo "downloaded pt: ${DOWNLOADED_PT}"

# -- Export fp32 ONNX at both sizes --------------------------------------------
# `ultralytics.YOLO.export` writes its output next to the .pt (same stem, .onnx).
# Exporting twice in the same interpreter requires copying out between runs
# because the second call overwrites the first.
"${PY}" - "${DOWNLOADED_PT}" "${OUT_1280_FP32}" "${OUT_640_FP32}" <<'PY'
import shutil, sys, os
from ultralytics import YOLO
pt, out1280, out640 = sys.argv[1], sys.argv[2], sys.argv[3]
exported = os.path.splitext(pt)[0] + ".onnx"

if not os.path.exists(out1280):
    YOLO(pt).export(format="onnx", imgsz=1280, simplify=True, opset=17)
    shutil.copyfile(exported, out1280)
    print(f"wrote {out1280}")
else:
    print(f"skip (exists): {out1280}")

if not os.path.exists(out640):
    YOLO(pt).export(format="onnx", imgsz=640, simplify=True, opset=17)
    shutil.copyfile(exported, out640)
    print(f"wrote {out640}")
else:
    print(f"skip (exists): {out640}")
PY

# -- Dynamic int8 quantization -------------------------------------------------
# Dynamic quantization of Conv is unreliable in some ORT versions; fall back to
# MatMul-only + QUInt8 if the first attempt fails.
"${PY}" - "${OUT_1280_FP32}" "${OUT_1280_INT8}" "${OUT_640_FP32}" "${OUT_640_INT8}" <<'PY'
import os, sys
from onnxruntime.quantization import quantize_dynamic, QuantType

pairs = [(sys.argv[1], sys.argv[2]), (sys.argv[3], sys.argv[4])]

def _quant(src, dst):
    if os.path.exists(dst):
        print(f"skip (exists): {dst}")
        return
    # First try: QInt8, MatMul+Conv.
    try:
        quantize_dynamic(
            src, dst,
            weight_type=QuantType.QInt8,
            op_types_to_quantize=["MatMul", "Conv"],
        )
        print(f"wrote {dst} (QInt8 MatMul+Conv)")
        return
    except Exception as e:
        print(f"QInt8 MatMul+Conv failed for {src}: {e!r}; retrying QUInt8 MatMul-only")
        if os.path.exists(dst):
            os.remove(dst)
    # Fallback: QUInt8, MatMul only.
    try:
        quantize_dynamic(
            src, dst,
            weight_type=QuantType.QUInt8,
            op_types_to_quantize=["MatMul"],
        )
        print(f"wrote {dst} (QUInt8 MatMul-only fallback)")
    except Exception as e:
        print(f"quantization failed for {src} -> {dst}: {e!r}", file=sys.stderr)
        raise

for src, dst in pairs:
    _quant(src, dst)
PY

echo ""
echo "done. artifacts:"
for f in "${OUT_1280_FP32}" "${OUT_640_FP32}" "${OUT_1280_INT8}" "${OUT_640_INT8}"; do
    if [[ -f "$f" ]]; then
        printf "  %-48s %12d bytes\n" "$f" "$(stat -c%s "$f")"
    else
        printf "  %-48s MISSING\n" "$f"
    fi
done
