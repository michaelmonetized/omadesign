#!/usr/bin/env python3
"""Reproduce the pinned fp32 CPU models from BSD-licensed upstream weights.

Based on Real-ESRGAN scripts/pytorch2onnx.py: load params/params_ema, eval, export.
Extends it to SRVGG/native x2/anime and dynamic spatial axes. Build-time only.
Python 3.14, torch 2.9.1, onnx 1.20.0, numpy 2.4.1.
Usage: python scripts/convert-upscale-models.py [output-directory]
"""
import ast
import hashlib
import json
from pathlib import Path
import sys
import urllib.request

import onnx
import torch
from torch import nn
from torch.nn import functional as F, init

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "assets/models/upscale-manifest.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def fetch(item, directory):
    path = directory / item["url"].split("/")[-1]
    if not path.exists() or digest(path.read_bytes()) != item["sha256"]:
        data = urllib.request.urlopen(item["url"], timeout=120).read()
        if digest(data) != item["sha256"]:
            raise RuntimeError(f"Upstream verification failed: {path.name}")
        path.write_bytes(data)
    return path.read_bytes()


def architecture(sources):
    # Execute only the upstream architecture definitions needed for inference.
    # BasicSR's package initializer otherwise imports its entire training stack.
    namespace = dict(torch=torch, nn=nn, F=F, init=init)
    for name in ["arch_util", "rrdbnet_arch", "srvgg_arch"]:
        tree = ast.parse(sources[name])
        nodes = []
        for node in tree.body:
            if isinstance(node, ast.FunctionDef) and node.name in {
                "default_init_weights", "make_layer", "pixel_unshuffle"
            }:
                nodes.append(node)
            elif isinstance(node, ast.ClassDef) and name != "arch_util":
                node.decorator_list = []  # Training registry only.
                nodes.append(node)
        exec(compile(ast.Module(body=nodes, type_ignores=[]), name, "exec"), namespace)
    return namespace


def main():
    output = Path(sys.argv[1] if len(sys.argv) > 1 else "target/upscale-models")
    output.mkdir(parents=True, exist_ok=True)
    spec = json.loads(MANIFEST.read_text())
    sources = {k: fetch(v, output).decode() for k, v in spec["sources"].items()}
    definitions = architecture(sources)
    torch.set_num_threads(4)
    torch.manual_seed(0)
    results = []
    for item in spec["models"]:
        weights = fetch(item["weights"], output)
        import io
        checkpoint = torch.load(io.BytesIO(weights), map_location="cpu", weights_only=True)
        model = definitions[item["architecture"]](**item["settings"])
        model.load_state_dict(checkpoint[item["state_key"]], strict=True)
        model.cpu().eval()
        path = output / item["filename"]
        with torch.no_grad():
            torch.onnx.export(
                model, torch.rand(1, 3, 16, 20), str(path),
                export_params=True, opset_version=17, dynamo=False,
                input_names=["input"], output_names=["output"],
                dynamic_axes={"input": {2: "height", 3: "width"},
                              "output": {2: "out_height", 3: "out_width"}},
            )
        onnx.checker.check_model(onnx.load(path))
        data = path.read_bytes()
        result = {"filename": path.name, "size": len(data), "sha256": digest(data)}
        if "sha256" in item and result["sha256"] != item["sha256"]:
            raise RuntimeError(f"Export is not reproducible: {result}")
        results.append(result)
        print(json.dumps(result), flush=True)
    (output / "converted.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
