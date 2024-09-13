# AlephResearch

AlephResearch is a comprehensive project aimed at enhancing the Aleph protocol's communication complexity. The project is organized into two primary branches, each focusing on different aspects of the Aleph protocol:

- **`aleph-orig`**: Implements the original Aleph protocol using Merkle trees in its Reliable Broadcast Communication (RBC) protocol. This branch serves as the baseline for performance comparison and benchmarking.

- **`aleph-rsa`**: Upgrades the Aleph protocol by replacing Merkle trees with RSA accumulators in the RBC protocol. This modification aims to reduce communication complexity and improve performance.

## Branches

To explore the specific work related to each sub-project, you need to check out the respective branches:

1. **`aleph-orig` Branch**: 
   - Implements the original Aleph protocol with Merkle trees.
   - To view or work on this branch, use the following command:
     ```bash
     git checkout aleph-orig
     ```

2. **`aleph-rsa` Branch**:
   - Implements the upgraded Aleph protocol with RSA accumulators.
   - To view or work on this branch, use the following command:
     ```bash
     git checkout aleph-rsa
     ```

## Overview

AlephResearch explores the efficiency of the Aleph protocol under different configurations, focusing on reducing communication overhead and enhancing scalability. The project involves implementing and comparing the original and modified protocols to assess improvements in transaction throughput, latency, and resource utilization.

For more detailed information and instructions on each branch, please refer to the respective branch's README file.

