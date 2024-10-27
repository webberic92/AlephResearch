from aws_cdk import (
    aws_ec2 as ec2,
    aws_logs as logs,
    aws_iam as iam,
    Stack,
    CfnOutput,
    App,
)
from constructs import Construct
from datetime import datetime

class TestAleph(Stack):
    def __init__(self, scope: Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        INSTANCES_NUMBER = 2  # Define the number of instances
        TRANSACTIONS_PER_NODE = 4  # Define the number of transactions per node for the test
        unique_id = datetime.now().strftime("%Y%m%d%H%M")

        # Create a VPC within the scope of this Stack
        vpc = ec2.Vpc(self, "MyVpc", max_azs=2)

        # Create a log group for CloudWatch logging
        log_group = logs.LogGroup(self, "AlephNodeLogGroup", log_group_name=f"/aleph-research/nodes-{unique_id}")

        # Create a security group for EC2 instances with intra-VPC communication
        security_group = ec2.SecurityGroup(
            self, "AlephNodeSG",
            vpc=vpc,
            allow_all_outbound=True
        )
        security_group.add_ingress_rule(ec2.Peer.ipv4(vpc.vpc_cidr_block), ec2.Port.all_traffic(), "Allow VPC-wide communication")

        # IAM Role for EC2 Instances (for S3, CloudWatch, and SSM)
        instance_role = iam.Role(
            self, "InstanceRole",
            assumed_by=iam.ServicePrincipal("ec2.amazonaws.com")
        )
        # Attach policies for S3, CloudWatch Logs, and SSM Session Manager
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("CloudWatchLogsFullAccess"))
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("AmazonS3ReadOnlyAccess"))
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("AmazonSSMManagedInstanceCore"))

        # Define a lightweight t2.micro instance as the IP Manager
        ip_manager_instance = ec2.Instance(self, "IPManager",
                                           instance_type=ec2.InstanceType("t2.micro"),
                                           machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                           vpc=vpc,
                                           security_group=security_group,
                                           role=instance_role
        )
        # Start the IP server for readiness tracking
        ip_manager_instance.user_data.add_commands(
            "sudo yum update -y",
            "sudo yum install -y python3 jq",
            "aws s3 cp s3://aleph-research/IpServer.py /home/ec2-user/ --quiet",
            "sudo chmod -R 777 /home/ec2-user",
            "cd /home/ec2-user", 
            "python3 IpServer.py &"
        )

        # Define the EC2 instances for Aleph nodes
        for i in range(INSTANCES_NUMBER):
            ec2_instance = ec2.Instance(self, f"MyInstance{i+1}",
                                        instance_type=ec2.InstanceType("t3.medium"),
                                        machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                        vpc=vpc,
                                        security_group=security_group,
                                        role=instance_role
            )

            # Install dependencies, download binaries, and configure instance
            ec2_instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git jq python3 awslogs amazon-ssm-agent aws-cli",
                "aws s3 cp s3://aleph-research/alephRBC /home/aleph-node/ --quiet",
                "aws s3 cp s3://aleph-research/generate_keys /home/aleph-node/ --quiet",
                "sudo chmod -R 777 /home/aleph-node/",  # Make binaries executable

                # Create necessary logs
                "mkdir -p /home/aleph-node/logs/",
                "touch /home/aleph-node/logs/resource_usage",
                "touch /home/aleph-node/logs/node_status",
                "touch /home/aleph-node/logs/error_logs",
                "touch /home/aleph-node/logs/transaction_metrics",
                "touch /home/aleph-node/logs/network_metrics",
                "chmod -R 777 /home/aleph-node/logs/",

                # Initialize node_status log
                "echo 'Node Status init test...' >> /home/aleph-node/logs/node_status",
                f"echo 'Registering node IP with IP Manager.' >> /home/aleph-node/logs/node_status",

                # Register node as ready with the IP manager
                f"curl -X POST -H 'Content-Type: application/json' -d '{{\"node_ip\": \"$(curl -s http://169.254.169.254/latest/meta-data/local-ipv4)\"}}' http://{ip_manager_instance.instance_private_ip}:8080/node_ready",

                # Loop to check IP Manager endpoint readiness
                "while true; do",
                f"  if curl -s http://{ip_manager_instance.instance_private_ip}:8080/check_all_ready | grep -q '\"all_ready\": true'; then",
                "    echo 'IP Manager is reachable and all nodes are ready.' >> /home/aleph-node/logs/node_status;",
                "    break;",  # Exit loop if IP Manager is reachable and all nodes are ready
                "  else",
                "    echo 'IP Manager not ready, retrying...' >> /home/aleph-node/logs/node_status;",
                "  fi",
                "  sleep 5;",  # Wait before retrying
                "done",

                "echo 'About to start alephRBC' >> /home/aleph-node/logs/node_status",

                # Start the Aleph node with the configuration file
                "/home/ec2-user/alephRBC --config /home/aleph-node/aleph-node-config.toml",

                # Log resource usage every 5 seconds
                "while true; do",
                "  top -b -n1 | grep 'Cpu(s)' >> /home/aleph-node/logs/resource_usage",
                "  free -m >> /home/aleph-node/logs/resource_usage",
                "  sleep 5;",  # Log every 5 seconds
                "done &",

                # Sync logs to S3 after the test
                f"aws s3 sync /home/aleph-node/logs s3://aleph-research/{INSTANCES_NUMBER}nodes_{TRANSACTIONS_PER_NODE}transactions/instance-{i+1}/ --quiet"
            )

            # Output the instance ID for debugging
            CfnOutput(self, f"InstanceIdOutput{i+1}",
                      value=ec2_instance.instance_id,
                      description=f"Instance ID for MyInstance{i+1}")

# App setup
app = App()
TestAleph(app, "TestAleph")  # Instantiate the TestAleph Stack within the app

app.synth()
