#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
from __future__ import annotations
import argparse, hashlib, json, math, struct
from pathlib import Path

MAGIC=b"NSM1"; SCHEMA_VERSION=1; Q_SHIFT=12

def feature_index(name, dimensions):
    digest=hashlib.sha256(name.encode("utf-8")).digest()
    return 1 + int.from_bytes(digest[:8], "little") % (dimensions-1)

def read_dataset(path):
    rows=[]
    for line_no,line in enumerate(path.read_text().splitlines(),1):
        if not line.strip(): continue
        row=json.loads(line); target=row.get("teacher_probability",row.get("label"))
        if target is None or not 0.0 <= float(target) <= 1.0:
            raise ValueError(f"line {line_no}: invalid target")
        rows.append((row["features"],float(target)))
    if not rows: raise ValueError("dataset is empty")
    return rows

class FTRL:
    def __init__(self,d,alpha=.08,beta=1.,l1=.05,l2=1.):
        self.d=d; self.alpha=alpha; self.beta=beta; self.l1=l1; self.l2=l2
        self.z=[0.]*d; self.n=[0.]*d
    def weight(self,i):
        z=self.z[i]
        if abs(z)<=self.l1: return 0.
        sign=-1. if z<0 else 1.
        return -(z-sign*self.l1)/((self.beta+math.sqrt(self.n[i]))/self.alpha+self.l2)
    def vectorize(self,raw):
        x={0:1.}
        for name,value in raw.items():
            value=float(value)
            if value:
                idx=feature_index(name,self.d); x[idx]=x.get(idx,0.)+value
        return x
    def predict(self,x):
        s=sum(self.weight(i)*v for i,v in x.items()); s=max(-35.,min(35.,s))
        return 1./(1.+math.exp(-s))
    def update(self,x,y):
        p=self.predict(x)
        for i,v in x.items():
            g=(p-y)*v; w=self.weight(i)
            sigma=(math.sqrt(self.n[i]+g*g)-math.sqrt(self.n[i]))/self.alpha
            self.z[i]+=g-sigma*w; self.n[i]+=g*g

def write_nsm(path,weights):
    q=[max(-32768,min(32767,int(round(w*(1<<Q_SHIFT))))) for w in weights]
    path.write_bytes(MAGIC+struct.pack("<HHB3x",SCHEMA_VERSION,len(q),Q_SHIFT)+struct.pack("<"+"h"*len(q),*q))

def main():
    p=argparse.ArgumentParser(); p.add_argument("--dataset",type=Path,required=True)
    p.add_argument("--output-dir",type=Path,required=True); p.add_argument("--dimensions",type=int,default=128)
    p.add_argument("--epochs",type=int,default=40); a=p.parse_args()
    rows=read_dataset(a.dataset); m=FTRL(a.dimensions)
    for _ in range(a.epochs):
        for raw,y in rows: m.update(m.vectorize(raw),y)
    weights=[m.weight(i) for i in range(a.dimensions)]
    a.output_dir.mkdir(parents=True,exist_ok=True)
    (a.output_dir/"student.json").write_text(json.dumps({"format":"nasaru-ftrl-v1","schema_version":1,"dimensions":a.dimensions,"q_shift":Q_SHIFT,"weights":weights},indent=2)+"\n")
    write_nsm(a.output_dir/"student.nsm",weights)

if __name__=="__main__": main()
