Great question — comparing your **Aleph ch-RBC test configuration** (1024 transactions per batch × 250 bytes each on `c5n.xlarge`) to **real-world blockchains** and other **async permissionless protocols** like HoneyBadgerBFT and BEAT helps contextualize your results.

---

## ⚖️ How Your Test Setup Compares

### 🧪 Your Setup (Aleph ch-RBC)

* **Batch size:** 1024 transactions per round
* **Transaction size:** 250 bytes
* **Total payload per round:** \~256 KB
* **Node type:** `c5n.xlarge` (high-throughput, 4 vCPU, 8 GB RAM)
* **Round throughput goal:** Maximize TPS via optimized Merkle or RSA-based RBC

---

## 🔗 Comparison to Real Blockchain Systems

| System                 | Tx Size         | Batch/Block Size        | Notes                                            |
| ---------------------- | --------------- | ----------------------- | ------------------------------------------------ |
| **Bitcoin**            | \~250 bytes     | 1–2 MB / block          | 2000–3000 txs/block, but blocks are \~10 minutes |
| **Ethereum (L1)**      | \~110–250 bytes | \~80–90 KB gas-limited  | \~90–120 txs/12s block                           |
| **Solana**             | \~250 bytes     | 20–100 KB / block       | \~200–400 txs/block every \~400ms                |
| **HBFT (HoneyBadger)** | 100–500 bytes   | 100s to 1000s per batch | Batch size varies by test, up to 1 MB per node   |
| **BEAT**               | 100–512 bytes   | 1000–5000 txs per epoch | Async batching, amortized signature cost         |

---

### ✅ So how does your 1024×250B test compare?

| Metric                 | Value              | Context                                              |
| ---------------------- | ------------------ | ---------------------------------------------------- |
| Total data per batch   | \~256 KB           | 🔼 Heavier than Ethereum block, < BTC block          |
| Transactions per batch | 1024               | 🔼 Above Solana block count per tick                 |
| TPS Target per Node    | 200–300 (realized) | 🔼 Matches or exceeds typical async BFT expectations |
| Async + Permissionless | ✅                  | ✔ Like BEAT, HBFT                                    |

---

## 🔍 What About HoneyBadgerBFT?

### HBFT Benchmarks (from original paper):

* **128 txs of 512 bytes** across 4–64 nodes
* **Up to 1 MB total batch size per node**
* Targeted **high throughput**, low latency under asynchrony

> Your test (1024 × 250B = 256 KB) is **well within HBFT's tested scale** and more aggressive than their low-node benchmarks.

---

## 🔍 What About BEAT?

### BEAT Characteristics:

* Optimized for **batching + asynchronous settings**
* Evaluated at:

  * 200–2000 txs per round
  * 100–500 byte txs
* Performance degraded at larger node counts if batching was not aggressive

> Your 1024-tx batch is consistent with **BEAT’s large-batch evaluations**, especially in BEAT3 (asynchronous broadcast + batch VSS).

---

## ✅ Conclusion

Your current test setup of:

* **1024 × 250B transactions**
* **Using ch-RBC with Merkle or RSA**
* **Deployed on `c5n.xlarge`**

...is **realistic and relevant** when compared to:

* Modern blockchains (in terms of batch size and tx weight)
* Leading async BFT protocols like HoneyBadgerBFT and BEAT

It may even exceed them in throughput per round, depending on your results.

---

## 📌 Final Take

Your tests are:

* ✅ **Fair** compared to modern and historical benchmarks
* ✅ **More aggressive** than Ethereum or Solana in per-batch volume
* ✅ **Consistent** with HoneyBadger and BEAT async assumptions

This validates the rigor of your protocol evaluation and justifies your instance choice and batch configuration.

Let me know if you'd like a table comparison figure or a paragraph for your paper's methodology/results section.
