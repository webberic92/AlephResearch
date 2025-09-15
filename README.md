# AlephResearch

*AlephResearch* is a comprehensive project aimed at enhancing the Aleph protocol's communication complexity. **This work was developed as part of a PhD in Cybersecurity Management at [Nova Southeastern University](https://www.nova.edu).**

The project is organized into two primary branches, each focusing on different aspects of the Aleph protocol:

* **`aleph-merkle`**: Implements the original Aleph protocol using Merkle trees in its Reliable Broadcast Communication (RBC) protocol. This branch serves as the baseline for performance comparison and benchmarking.

* **`aleph-rsa`**: Upgrades the Aleph protocol by replacing Merkle trees with RSA accumulators in the RBC protocol which allows for batching and parallelization. This modification aims to reduce communication complexity and improve performance.

## Branches

To explore the specific work related to each sub-project, you need to check out the respective branches:

1. **`aleph-merkle` Branch**:

   * Implements the original Aleph protocol with Merkle trees.
   * To view or work on this branch, use the following command:

     ```bash
     git checkout aleph-merkle
     ```

2. **`aleph-rsa` Branch**:

   * Implements the upgraded Aleph protocol with RSA accumulators.
   * To view or work on this branch, use the following command:

     ```bash
     git checkout aleph-rsa
     ```

## AWS Setup

Running large-scale experiments requires AWS resources. Before deploying:

1. **Create an IAM user** (e.g., `alephDeployer`) with programmatic access.
   Attach at minimum the following policies:

   * `AmazonEC2FullAccess`
   * `AmazonEC2ContainerRegistryFullAccess`
   * `AmazonS3FullAccess`
   * `AmazonSSMFullAccess`
   * `CloudWatchLogsFullAccess`
   * `AWSCloudFormationFullAccess`
     *(or use a more restrictive custom policy if you prefer)*

2. **Generate an access key** for that user and configure it locally:

   ```bash
   aws configure
   # supply aws_access_key_id and aws_secret_access_key
   ```

3. **Increase EC2 service quotas** for the instance families used in benchmarking:

   * `c5n.*` (e.g., `c5n.xlarge`)
   * `t3.*`
     Go to **Service Quotas → EC2 → Running On-Demand instances**, request higher limits for those instance types in your region.

4. Make sure the IAM key you use for deployments is kept out of version control (add `~/.aws/credentials` to `.gitignore` and never commit it).

## Overview

*AlephResearch* explores the efficiency of the Aleph protocol under different configurations, focusing on reducing communication overhead and enhancing scalability. The project involves implementing and comparing the original and modified protocols to assess improvements in transaction throughput, latency, and resource utilization.


