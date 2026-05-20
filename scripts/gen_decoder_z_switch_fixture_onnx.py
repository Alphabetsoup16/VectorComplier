#!/usr/bin/env python3
"""Regenerate benchmarks/fixtures/decoder_z_switch.onnx.

Toy decoder: ``program_ir_json`` depends on ``z[0, 0]`` (first latent coefficient).

  - ``z[0]`` (first column of batch row 0, via Gather axis=1) ``>= 0`` → add.vcir
  - else            → UTF-8 JSON bytes of benchmarks/programs/mul.vcir

Auxiliary output ``y`` = Abs(``z``). Both embedded IR payloads are the same length
(320 bytes today) so ``Where`` can select element-wise.

Uses ONNX opset 13 with **ModelProto.ir_version = 10** (ORT 1.22 compatible).

Requires: pip install onnx numpy
"""
from pathlib import Path

import numpy as np
import onnx
from onnx import TensorProto, helper


def _uint8_constant(name: str, out: str, data: bytes) -> onnx.NodeProto:
    arr = np.frombuffer(data, dtype=np.uint8)
    tensor_proto = onnx.numpy_helper.from_array(arr)
    return helper.make_node("Constant", [], [out], value=tensor_proto, name=name)


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    add_bytes = (root / "benchmarks/programs/add.vcir").read_bytes()
    mul_bytes = (root / "benchmarks/programs/mul.vcir").read_bytes()
    if len(add_bytes) != len(mul_bytes):
        raise SystemExit(
            f"add.vcir ({len(add_bytes)} B) and mul.vcir ({len(mul_bytes)} B) must be "
            "the same length for element-wise Where"
        )

    n_ir = len(add_bytes)
    d = 256

    z_info = helper.make_tensor_value_info("z", TensorProto.FLOAT, [1, d])
    y_info = helper.make_tensor_value_info("y", TensorProto.FLOAT, [1, d])
    ir_info = helper.make_tensor_value_info(
        "program_ir_json", TensorProto.UINT8, [n_ir]
    )

    node_y = helper.make_node("Abs", ["z"], ["y"], name="abs_aux")

    idx0 = onnx.numpy_helper.from_array(np.array(0, dtype=np.int64))
    node_idx0 = helper.make_node("Constant", [], ["idx0"], value=idx0, name="const_idx0")
    node_z0 = helper.make_node(
        "Gather", ["z", "idx0"], ["z0"], axis=1, name="gather_z_col0"
    )

    zero = onnx.numpy_helper.from_array(np.array(0.0, dtype=np.float32))
    node_zero = helper.make_node("Constant", [], ["zero"], value=zero, name="const_zero")

    node_ge = helper.make_node(
        "GreaterOrEqual", ["z0", "zero"], ["use_add"], name="ge_z0_nonneg"
    )

    node_ir_add = _uint8_constant("ir_add_constant", "ir_add", add_bytes)
    node_ir_mul = _uint8_constant("ir_mul_constant", "ir_mul", mul_bytes)

    node_where = helper.make_node(
        "Where", ["use_add", "ir_add", "ir_mul"], ["program_ir_json"], name="pick_ir"
    )

    graph = helper.make_graph(
        [
            node_y,
            node_idx0,
            node_z0,
            node_zero,
            node_ge,
            node_ir_add,
            node_ir_mul,
            node_where,
        ],
        "decoder_z_switch_fixture",
        [z_info],
        [y_info, ir_info],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 10
    onnx.checker.check_model(model)

    out = root / "benchmarks/fixtures/decoder_z_switch.onnx"
    out.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, str(out))
    print(
        f"wrote {out} ir_json_bytes={n_ir} ir_version={model.ir_version} "
        f"(add if z[0,0]>=0 else mul)"
    )


if __name__ == "__main__":
    main()
