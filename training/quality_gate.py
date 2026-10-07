#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse,json
from pathlib import Path
p=argparse.ArgumentParser(); p.add_argument("--metrics",type=Path,required=True); p.add_argument("--runtime-model",type=Path,required=True); p.add_argument("--max-false-allow",type=float,default=.20); p.add_argument("--max-false-block",type=float,default=.20); p.add_argument("--max-bytes",type=int,default=65536); a=p.parse_args()
m=json.loads(a.metrics.read_text()); failures=[]
if m["false_allow_rate"]>a.max_false_allow: failures.append(f"false_allow_rate={m['false_allow_rate']:.6f}")
if m["false_block_rate"]>a.max_false_block: failures.append(f"false_block_rate={m['false_block_rate']:.6f}")
size=a.runtime_model.stat().st_size
if size>a.max_bytes: failures.append(f"runtime_model_size={size}")
if failures: raise SystemExit("quality gate failed: "+", ".join(failures))
print(f"quality gate passed: size={size} bytes")
