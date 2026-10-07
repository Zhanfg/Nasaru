#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse, hashlib, json, math
from pathlib import Path

def idx(name,d):
    return 1+int.from_bytes(hashlib.sha256(name.encode()).digest()[:8],"little")%(d-1)

def main():
    p=argparse.ArgumentParser(); p.add_argument("--model",type=Path,required=True); p.add_argument("--dataset",type=Path,required=True); p.add_argument("--output",type=Path,required=True); a=p.parse_args()
    w=json.loads(a.model.read_text())["weights"]; tp=tn=fp=fn=0; losses=[]
    for line in a.dataset.read_text().splitlines():
        if not line.strip(): continue
        r=json.loads(line); y=float(r.get("teacher_probability",r["label"])); s=w[0]
        for name,value in r["features"].items(): s+=w[idx(name,len(w))]*float(value)
        s=max(-35.,min(35.,s)); prob=1./(1.+math.exp(-s)); pred=prob>=.5; truth=y>=.5
        losses.append(-(y*math.log(max(prob,1e-9))+(1-y)*math.log(max(1-prob,1e-9))))
        if pred and truth: tp+=1
        elif pred: fp+=1
        elif truth: fn+=1
        else: tn+=1
    pos=tp+fn; neg=tn+fp; total=pos+neg
    m={"samples":total,"false_allow_rate":fp/neg if neg else 0.,"false_block_rate":fn/pos if pos else 0.,"accuracy":(tp+tn)/total,"log_loss":sum(losses)/len(losses),"confusion":{"tp":tp,"tn":tn,"fp":fp,"fn":fn}}
    a.output.parent.mkdir(parents=True,exist_ok=True); a.output.write_text(json.dumps(m,indent=2)+"\n"); print(json.dumps(m,indent=2))
if __name__=="__main__": main()
