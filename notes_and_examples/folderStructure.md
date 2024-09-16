## Folders and Files

### `/infrastructure`

- **vpc_stack.py**: VPC setup with public and private subnets, Internet Gateway (IGW), and route tables.
  - **Notes**: Ensure VPC, subnets, IGW, and route tables are configured correctly based on your network requirements.
  - **What’s Missing/Needed**: Verify subnet CIDR blocks, IGW configuration, and route table associations. Adjust the configuration for specific networking needs.

- **ec2_instances_stack.py**: EC2 instance provisioning and setup, including user data for dependencies.
  - **Notes**: This script provisions EC2 instances with the necessary setup for your environment.
  - **What’s Missing/Needed**: Specify instance types, AMIs, and user data for installing necessary software. Ensure that the instance configurations meet your workload requirements.

- **security_groups.py**: Security group definitions and rules for EC2 instances.
  - **Notes**: Security groups control inbound and outbound traffic for EC2 instances.
  - **What’s Missing/Needed**: Define appropriate rules for your instances, including ports and protocols. Ensure security groups allow necessary traffic for application communication.

### `/aleph_deployment`

- **aleph_deployment_stack.py**: Deployment of Aleph protocol on EC2 instances, GitHub cloning, compiling, and configuration.
  - **Notes**: This script handles the deployment process, including cloning the GitHub repository and setting up the Aleph protocol.
  - **What’s Missing/Needed**: Provide the GitHub repository details, including repository name and owner. Ensure commands for cloning, compiling, and configuring are accurate and complete.

- **transaction_proposal_setup.py**: Setup of nodes for transaction proposals and logging configuration.
  - **Notes**: Configures nodes to propose transactions and sets up logging for capturing relevant metrics.
  - **What’s Missing/Needed**: Specify node configurations and ensure logging setup aligns with your testing and monitoring requirements.

### `/monitoring_and_logging`

- **monitoring_stack.py**: Setup for CloudWatch, Prometheus Node Exporter, or other monitoring tools.
  - **Notes**: Configures monitoring tools to track metrics and performance of EC2 instances.
  - **What’s Missing/Needed**: Decide on the monitoring tools (CloudWatch, Prometheus) and configure them appropriately. Ensure the setup includes metrics relevant to your needs.

- **logging_configuration.py**: Logging setup for capturing throughput, latency, and communication overhead in structured formats.
  - **Notes**: Sets up logging to capture important metrics and store them in a structured format.
  - **What’s Missing/Needed**: Ensure logging configuration captures all required metrics and outputs them in a format suitable for analysis (e.g., JSON, CSV).

### `/database_and_storage`

- **rds_stack.py**: RDS instance provisioning and schema creation for storing logs.
  - **Notes**: Provisions an RDS instance and sets up the schema for storing log data.
  - **What’s Missing/Needed**: Confirm database engine (PostgreSQL or MySQL), instance types, and schema structure. Ensure the RDS instance is accessible from EC2 instances.

- **log_ingestion_script.py**: Script for periodic log ingestion from EC2 to RDS database.
  - **Notes**: Periodically ingests logs from EC2 instances into the RDS database.
  - **What’s Missing/Needed**: Implement log ingestion logic and ensure that the script handles data transfer securely and efficiently. Define the schedule for periodic log ingestion.

### `/automation`

- **shutdown_script.py**: Script to automatically shut down EC2 instances after tests.
  - **Notes**: Automates the shutdown of EC2 instances to avoid incurring unnecessary costs.
  - **What’s Missing/Needed**: Ensure the script properly stops or terminates instances. Verify that it handles instances securely and correctly.

- **shutdown_scheduler.py**: Setup for AWS Systems Manager or CloudWatch Events to trigger shutdown script.
  - **Notes**: Configures automated scheduling for running the shutdown script.
  - **What’s Missing/Needed**: Set up AWS Systems Manager or CloudWatch Events to trigger the shutdown script based on test completion. Verify scheduling and trigger conditions.

### `/testing_and_analysis`

- **test_runner.py**: Script or program to run tests, start nodes, and initiate transactions.
  - **Notes**: Manages the execution of tests, including starting nodes and initiating transactions.
  - **What’s Missing/Needed**: Provide the command or method to start your test. Adjust monitoring based on your test requirements and duration.

- **results_analysis.py**: SQL queries and BI tool configurations for analyzing test results.
  - **Notes**: Analyzes test results using SQL queries and prepares data for visualization with BI tools.
  - **What’s Missing/Needed**: Verify SQL queries based on your log schema. Specify BI tools for visualization and configure accordingly.

### `/ci_cd`

- **ci_cd_pipeline.py**: CI/CD pipeline setup using AWS CodePipeline or GitHub Actions.
  - **Notes**: Sets up an automated CI/CD pipeline for deployment using AWS CodePipeline or GitHub Actions.
  - **What’s Missing/Needed**: Provide GitHub repository details, specify deployment targets, and configure the pipeline stages accordingly.

- **buildspec.yml**: Build specifications for CodeBuild, used in the CI/CD process.
  - **Notes**: Defines the build commands and phases for AWS CodeBuild.
  - **What’s Missing/Needed**: Adjust build commands and dependencies according to your project requirements. Ensure the buildspec aligns with your CI/CD process.

### Root Files

- **app.py**: Main entry point for the CDK application, initializes all stacks.
  - **Notes**: Initializes and synthesizes all CDK stacks.
  - **What’s Missing/Needed**: Ensure that all stack imports and initializations are correctly configured. Adjust stack names and identifiers as needed.

## Getting Started

To get started with this project, make sure you have the AWS CDK installed and configured. Follow the instructions in each file to set up and deploy the necessary resources for running the Aleph protocol.
