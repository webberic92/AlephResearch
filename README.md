# AlephResearch

AlephResearch is a comprehensive project aimed at enhancing the Aleph protocol's communication complexity. It includes two primary sub-projects:

- **aleph-orig**: Implements the original Aleph protocol using Merkle trees in its Reliable Broadcast Communication (RBC) protocol. This sub-project serves as the baseline for performance comparison and benchmarking.

- **aleph-rsa**: Upgrades the Aleph protocol by replacing Merkle trees with RSA accumulators in the RBC protocol. This modification aims to reduce communication complexity and improve performance.

## Overview

AlephResearch explores the efficiency of the Aleph protocol under different configurations, focusing on reducing communication overhead and enhancing scalability. The project involves implementing and comparing the original and modified protocols to assess improvements in transaction throughput, latency, and resource utilization.

## Sub-Projects

For detailed instructions and implementation specifics, refer to the individual README files in the sub-project directories:

- [aleph-orig](./aleph-orig/README.md): Instructions for setting up, running, and evaluating the original Aleph protocol.

- [aleph-rsa](./aleph-rsa/README.md): Guidelines for implementing, running, and assessing the Aleph protocol with RSA accumulators.

## Getting Started

1. **Clone the Repository**
   ```sh
   git clone <repository-url>
   cd AlephResearch
