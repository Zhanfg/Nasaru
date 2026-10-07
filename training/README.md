# Nasaru Micro training

GitHub Actions trains only the generic student. Personal raw events, feedback, explicit rules, device secrets and user adapters are not CI inputs.

The bootstrap model is dependency-free FTRL-Proximal with deterministic SHA-256 feature hashing and a compact Q12 integer runtime format (`.nsm`). The smoke dataset validates plumbing only; it is not a production corpus.

A later teacher-distillation job can produce versioned generic datasets. On-device personalization remains a separate adapter and never gets overwritten by a generic model update.
