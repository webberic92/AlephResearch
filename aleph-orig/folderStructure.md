
## Folders and Files

### `/infrastructure`
- **vpc_stack.py**: VPC setup with public and private subnets, Internet Gateway (IGW), and route tables.
- **ec2_instances_stack.py**: EC2 instance provisioning and setup, including user data for dependencies.
- **security_groups.py**: Security group definitions and rules for EC2 instances.

### `/aleph_deployment`
- **aleph_deployment_stack.py**: Deployment of Aleph protocol on EC2 instances, GitHub cloning, compiling, and configuration.
- **transaction_proposal_setup.py**: Setup of nodes for transaction proposals and logging configuration.

### `/monitoring_and_logging`
- **monitoring_stack.py**: Setup for CloudWatch, Prometheus Node Exporter, or other monitoring tools.
- **logging_configuration.py**: Logging setup for capturing throughput, latency, and communication overhead in structured formats.

### `/database_and_storage`
- **rds_stack.py**: RDS instance provisioning and schema creation for storing logs.
- **log_ingestion_script.py**: Script for periodic log ingestion from EC2 to RDS database.

### `/automation`
- **shutdown_script.py**: Script to automatically shut down EC2 instances after tests.
- **shutdown_scheduler.py**: Setup for AWS Systems Manager or CloudWatch Events to trigger shutdown script.

### `/testing_and_analysis`
- **test_runner.py**: Script or program to run tests, start nodes, and initiate transactions.
- **results_analysis.py**: SQL queries and BI tool configurations for analyzing test results.

### `/ci_cd`
- **ci_cd_pipeline.py**: CI/CD pipeline setup using AWS CodePipeline or GitHub Actions.
- **buildspec.yml**: Build specifications for CodeBuild, used in the CI/CD process.

### Root Files
- **app.py**: Main entry point for the CDK application, initializes all stacks.

## Getting Started

To get started with this project, make sure you have the AWS CDK installed and configured. Follow the instructions in each file to set up and deploy the necessary resources for running the Aleph protocol.

