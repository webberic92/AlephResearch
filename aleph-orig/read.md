# Aleph Protocol Setup and Deployment Guide

## Table of Contents

1. [Set Up Rust Environment](#1-set-up-rust-environment)
2. [Install and Configure PostgreSQL](#2-install-and-configure-postgresql)
3. [Run the Rust Program](#3-run-the-rust-program)
4. [Set Up AWS CDK for Infrastructure Deployment](#4-set-up-aws-cdk-for-infrastructure-deployment)
5. [Analyze Metrics with Jupyter Notebooks](#5-analyze-metrics-with-jupyter-notebooks)

## 1. Set Up Rust Environment

To implement and test the original Aleph protocol with Merkle trees, follow these steps to set up your Rust development environment.

### Steps:

1. **Install Rust and Cargo:**
   - If Rust is not installed, download it from [Rust's official website](https://www.rust-lang.org/tools/install).
   - Run the following command to install Rust and Cargo:
     ```sh
     curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
     ```
   - Follow the on-screen instructions and restart your terminal.

2. **Verify Rust Installation:**
   - Check if Rust and Cargo are installed correctly:
     ```sh
     rustc --version
     cargo --version
     ```
   - Both commands should output the installed versions of Rust and Cargo.

## 2. Install and Configure PostgreSQL

The Rust program uses PostgreSQL to log metrics such as transaction throughput, latency, and communication overhead. Follow these steps to set up PostgreSQL.

### Steps:

1. **Install PostgreSQL:**
   - **Locally:** Follow the instructions on [PostgreSQL's official website](https://www.postgresql.org/download/) for your operating system.
   - **On AWS RDS:** Set up PostgreSQL on AWS RDS following the AWS [RDS documentation](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/USER_CreatePostgreSQLInstance.html).

2. **Verify PostgreSQL Installation:**
   - Check PostgreSQL version:
     ```sh
     psql --version
     ```
   - Connect to the local PostgreSQL server to ensure it is running:
     ```sh
     psql -U postgres
     ```
   - Run `\l` to list databases and ensure you have the correct database set up.

## 3. Run the Rust Program

With Rust and PostgreSQL set up, you can now run the Rust program to implement the original Aleph protocol.

### Steps:

1. **Compile the Rust Code:**
   - Open your terminal in the root directory of the project.
   - Run the following command to build the project:
     ```sh
     cargo build
     ```

2. **Run the Rust Program:**
   - Execute the program with:
     ```sh
     cargo run
     ```
   - The program should start executing the Aleph protocol and log output to the console.

3. **Check Logging and Metrics:**
   - Ensure that logs are being output to the console via the `tracing` crate.
   - Verify that metrics are being recorded in the PostgreSQL `metrics` table:
     ```sql
     SELECT * FROM metrics;
     ```

## 4. Set Up AWS CDK for Infrastructure Deployment

We will use AWS Cloud Development Kit (CDK) to deploy the necessary infrastructure for running the Aleph protocol on AWS.

### Steps:

1. **Install AWS CDK:**
   - Install AWS CDK globally using npm:
     ```sh
     npm install -g aws-cdk
     ```

2. **Verify AWS CDK Installation:**
   - Check AWS CDK version:
     ```sh
     cdk --version
     ```

3. **Create a New CDK Application:**
   - Navigate to your project directory and initialize a new CDK application:
     ```sh
     mkdir aleph-cdk
     cd aleph-cdk
     cdk init app --language=typescript
     ```

4. **Add AWS CDK Dependencies:**
   - Install necessary AWS CDK dependencies:
     ```sh
     npm install @aws-cdk/aws-ec2 @aws-cdk/aws-rds @aws-cdk/aws-ssm constructs
     ```

5. **Create AWS Infrastructure:**
   - Use the provided `cdk_infrastructure.ts` code to define the infrastructure:
   - Open `lib/aleph-cdk-stack.ts` and replace its content with the AWS CDK code provided.

6. **Deploy the Infrastructure:**
   - Run the following command to deploy the CDK stack:
     ```sh
     cdk deploy
     ```
   - Follow the prompts to approve the deployment.

7. **Access Database Information:**
   - After deployment, use AWS Systems Manager (SSM) to retrieve the PostgreSQL endpoint and credentials created by the CDK.

## 5. Analyze Metrics with Jupyter Notebooks

Once the experiments are complete and metrics have been collected, use Jupyter Notebooks to analyze and visualize the data.

### Steps:

1. **Install Jupyter Notebooks:**
   - Install Jupyter Notebooks if you haven't already:
     ```sh
     pip install notebook
     ```

2. **Verify Jupyter Installation:**
   - Check Jupyter version:
     ```sh
     jupyter --version
     ```

3. **Start Jupyter Notebooks:**
   - Run the following command to start Jupyter:
     ```sh
     jupyter notebook
     ```
   - Open your browser to the provided URL.

4. **Connect to PostgreSQL Database:**
   - Use a Python library like `psycopg2` or `SQLAlchemy` to connect to the PostgreSQL database:
   ```python
   import psycopg2
   import pandas as pd

   # Connect to the PostgreSQL database
   conn = psycopg2.connect(
       host="your-db-host",
       database="aleph_protocol",
       user="postgres",
       password="your-db-password"
   )

   # Query the metrics table
   df = pd.read_sql("SELECT * FROM metrics", conn)

   # Display the data
   print(df)
