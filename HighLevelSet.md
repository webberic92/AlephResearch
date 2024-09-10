# Aleph Protocol Test Setup

## Updated Step-by-Step High-Level Setup:

### 1. Infrastructure Setup:

- **Provision a VPC:**
  - Create a Virtual Private Cloud (VPC) with public and private subnets.
  - Configure Internet Gateway (IGW) and Route Tables for public subnet internet access.

- **Launch Two EC2 Instances:**
  - Use AWS CDK or CloudFormation to launch two EC2 instances in the VPC.
  - Choose appropriate instance types (e.g., `t3.medium`) based on the expected workload.
  - Install necessary dependencies like Rust, Tokio, or any other relevant software.

- **Configure Security Groups:**
  - Set up security groups to allow inbound/outbound traffic on necessary ports (e.g., 22 for SSH, 8080 for application communication).
  - Ensure the two EC2 instances can communicate with each other securely.

### 2. Deploy the Aleph Protocol Implementation:

- **Install and Configure the Aleph Protocol on EC2 Instances:**
  - Clone the Aleph Protocol codebase from your GitHub repository to both EC2 instances.
  - Compile and configure the code to run with the parameters for testing (e.g., 256 transactions, 250 bytes each).

- **Setup Nodes for Transaction Proposals:**
  - Configure both EC2 instances as nodes that will propose transactions to each other.
  - Ensure the nodes are set up to log throughput, latency, communication overhead, and resource utilization.

### 3. Configure Logging and Monitoring:

- **Install Monitoring Tools:**
  - Use CloudWatch Agent or Prometheus Node Exporter on EC2 instances to collect CPU, memory, and network metrics.
  - Set up custom logging to capture transaction-related metrics like throughput, latency, and communication overhead.

- **Log Transaction Metrics:**
  - Develop a logging mechanism within the Aleph implementation to capture and log throughput (transactions per second), latency (time to complete transactions), communication overhead (bytes transferred), and resource utilization.
  - Ensure logs are in a structured format (e.g., JSON or CSV) for easier storage and analysis.

### 4. Set Up an RDS Database for Logs Storage:

- **Create an RDS Instance:**
  - Use AWS CDK or AWS Management Console to set up an Amazon RDS instance (PostgreSQL or MySQL) in the same VPC.
  - Configure the RDS security group to allow access from the EC2 instances.
  - Create a database schema to store logs: tables for throughput, latency, communication overhead, and resource utilization metrics.

### 5. Automate Log Collection and Storage:

- **Develop a Script or Use AWS SDK for Data Ingestion:**
  - Write a Python or Bash script to periodically pull logs from the EC2 instances.
  - Use the AWS SDK to insert logs into the RDS database.
  - Consider using a centralized log collection tool (e.g., Fluentd or Filebeat) to push logs from EC2 instances to AWS RDS automatically.

### 6. Run the Test:

- **Start the Nodes and Initiate Transactions:**
  - Run the Aleph protocol nodes on both EC2 instances.
  - Start the test by proposing 256 transactions of 250 bytes each between the two nodes.
  - Ensure that logging captures all relevant metrics.

### 7. Monitor the Test and Validate Logs:

- **Monitor Logs in Real-Time:**
  - Use CloudWatch Logs to monitor real-time logs for errors, performance issues, and transaction metrics.
  - Validate that all logs are being captured and stored correctly in the RDS database.

### 8. Automate EC2 Instance Shutdown After Test:

- **Create a Shutdown Script:**
  - Develop a script that runs after the test completes to shut down both EC2 instances.
  - Use the AWS CLI or AWS SDK within the script to stop or terminate the EC2 instances.
  - For example, use a command like `aws ec2 stop-instances --instance-ids i-1234567890abcdef0` to stop the instances.

- **Schedule Shutdown with AWS Systems Manager or CloudWatch Events:**
  - Use AWS Systems Manager Run Command or CloudWatch Events to trigger the shutdown script automatically after the test ends.
  - Ensure the shutdown is properly logged, and instances are terminated to avoid incurring costs.

### 9. Analyze and Visualize the Results:

- **Use SQL Queries or BI Tools for Analysis:**
  - Run SQL queries on the RDS database to analyze throughput, latency, communication overhead, and resource utilization.
  - Use a BI tool like Amazon QuickSight or Grafana to create dashboards for visualizing the test results.

### 10. Iterate and Scale:

- **Refine and Repeat Tests:**
  - Adjust parameters, such as transaction size, number of transactions, or instance types, and rerun tests.
  - Use findings from initial tests to scale up or optimize configurations.

### 11. Automate Deployment with CI/CD (Optional):

- **Set Up CI/CD Pipelines:**
  - Use AWS CodePipeline or GitHub Actions to automate the deployment of your infrastructure and Aleph protocol implementation.
  - Automate log collection, storage, and analysis processes.
