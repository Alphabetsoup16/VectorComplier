#!/usr/bin/env python3
"""Regenerate benchmarks/fixtures/decoder_identity_z.onnx.

Graph:
  - Input `z` [1, 256] f32.
  - Output `y` = Abs(z) — auxiliary float tensor.
  - Output `program_ir_json` = Constant: raw UTF-8 bytes of benchmarks/programs/add.vcir as uint8 tensor.

Uses ONNX opset 13 with **ModelProto.ir_version = 10** (ORT 1.22 compatible).

Requires: pip install onnx numpy
"""
from pathlib import Path

import numpy as np
import onnx
from onnx import TensorProto, helper


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    json_bytes = (root / "benchmarks/programs/add.vcir").read_bytes()

    d = 256
    z_info = helper.make_tensor_value_info("z", TensorProto.FLOAT, [1, d])
    y_info = helper.make_tensor_value_info("y", TensorProto.FLOAT, [1, d])
    ir_info = helper.make_tensor_value_info(
        "program_ir_json", TensorProto.UINT8, [len(json_bytes)]
    )

    arr = np.frombuffer(json_bytes, dtype=np.uint8)
    tensor_proto = onnx.numpy_helper.from_array(arr)
    node_ir = helper.make_node(
        "Constant", [], ["program_ir_json"], value=tensor_proto, name="ir_json_constant"
    )
    node_y = helper.make_node("Abs", ["z"], ["y"], name="abs_aux")

    graph = helper.make_graph(
        [node_y, node_ir],
        "decoder_ir_tensor_fixture",
        [z_info],
        [y_info, ir_info],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 10
    onnx.checker.check_model(model)
    out = root / "benchmarks/fixtures/decoder_identity_z.onnx"
    out.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, str(out))
    print(f"wrote {out} ir_json_bytes={len(json_bytes)} ir_version={model.ir_version}")


if __name__ == "__main__":
    main()
