from aws_cdk import (
    aws_ec2 as ec2,
    aws_logs as logs,
    aws_iam as iam,
    Stack,
    CfnOutput,
    App,
    Environment,
)
from constructs import Construct
from datetime import datetime

class TestAleph(Stack):
    def __init__(self, scope: Construct, id: str, vpc: ec2.Vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        INSTANCES_NUMBER = 4
        unique_id = datetime.now().strftime("%Y%m%d%H%M")

        # Create a log group for CloudWatch logging
        log_group = logs.LogGroup(self, "AlephNodeLogGroup", log_group_name=f"/aleph-research/nodes-{unique_id}")

        # Create a security group for EC2 instances
        security_group = ec2.SecurityGroup(
            self, "AlephNodeSG",
            vpc=vpc,
            allow_all_outbound=True
        )
        # Allow incoming traffic on TCP 30333 (node communication)
        security_group.add_ingress_rule(ec2.Peer.any_ipv4(), ec2.Port.tcp(30333), "Allow node communication on TCP 30333")

        # IAM Role for EC2 Instances (for terminating instances and pushing logs)
        instance_role = iam.Role(
            self, "InstanceRole",
            assumed_by=iam.ServicePrincipal("ec2.amazonaws.com")
        )
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("CloudWatchLogsFullAccess"))
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("AmazonEC2FullAccess"))

        # Define the EC2 instances
        for i in range(INSTANCES_NUMBER):
            instance = ec2.Instance(self, f"MyInstance{i+1}",
                                    instance_type=ec2.InstanceType("t3.medium"),
                                    machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                    vpc=vpc,
                                    security_group=security_group,
                                    key_name="alephResearch",
                                    role=instance_role
            )

            # Install dependencies and configure the instance with user data
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git cargo jq python3 awslogs amazon-ssm-agent",  # Install necessary packages
                "git clone https://github.com/webberic92/AlephResearch.git",  # Clone the Aleph repository
                "cd AlephResearch",
                "git checkout aleph-orig",  # Checkout the specific branch
                "cargo build --release",  # Build the project

                # Create necessary directories for logs
                "mkdir -p /home/aleph-node/logs/{memory_usage, node_status, error_logs, transaction_metrics, network_metrics}",
                "mkdir -p /home/aleph-node",

                # Generate keys for each node
                f"export NODE_INDEX={i+1} && ./target/release/generate_keys",  # Generate node-specific keys

                # Start node discovery and wait for discovery to complete
                "nohup ./target/release/node_discovery > /home/aleph-node/node_discovery.log 2>&1 &",
                "while [ ! -f /home/aleph-node/known_nodes.json ]; do sleep 1; done",  # Wait for known nodes to be written

                # Extract known node IPs for network configuration
                "KNOWN_NODES_IPS=$(jq -r '.[] | .ip_address' /home/aleph-node/known_nodes.json)",
                "if [ -z \"$KNOWN_NODES_IPS\" ]; then",
                "   NEW_IP=\"192.168.0.1\";",  # Default IP if no known nodes
                "else",
                "   LARGEST_IP=$(echo $KNOWN_NODES_IPS | tr ' ' '\\n' | sort -t '.' -k 4,4n | tail -1);",
                "   LAST_OCTET=$(echo $LARGEST_IP | awk -F '.' '{print $4}');",
                "   NEW_LAST_OCTET=$((LAST_OCTET + 1));",
                "   NEW_IP=$(echo $LARGEST_IP | sed 's/\\.[0-9]*$/.'$NEW_LAST_OCTET'/');",
                "fi",

                # Write network configuration to TOML file
                "echo '[network]' > /home/aleph-node/aleph-node-config.toml",
                "echo 'listen_address = \"/ip4/${NEW_IP}/tcp/30333\"' >> /home/aleph-node/aleph-node-config.toml",
                "if [ ! -z \"$KNOWN_NODES_IPS\" ]; then",
                "   BOOTNODE_ADDRESSES=$(cat /home/aleph-node/known_nodes.json | jq -r '.[] | .multiaddr' | tr '\\n' ' ');",
                "   echo 'bootnodes = [$BOOTNODE_ADDRESSES]' >> /home/aleph-node/aleph-node-config.toml;",
                "else",
                "   echo 'bootnodes = []' >> /home/aleph-node/aleph-node-config.toml;",
                "fi",

                # Add consensus and logging configuration
                "echo '[consensus]\\nbatch_size = 256\\ntimeout = 5000\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[logging]\\nlevel = \"info\"\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[metrics]\\nenabled = true\\n' >> /home/aleph-node/aleph-node-config.toml",

                # Start the Aleph node with the generated configuration file
                "./target/release/aleph-node --config /home/aleph-node/aleph-node-config.toml",

                # CloudWatch Logs setup
                "sudo tee /etc/awslogs/awslogs.conf << EOF",
                "[general]",
                "state_file = /var/lib/awslogs/agent-state",
                f"[/home/aleph-node/logs]",
                "file = /home/aleph-node/logs/*.log",
                f"log_group_name = {log_group.log_group_name}",
                f"log_stream_name = MyInstance{i+1}/aleph-node-log",
                "datetime_format = %Y-%m-%d %H:%M:%S",
                "EOF",
                "sudo systemctl start awslogsd"
            )

            # Output the instance ID for debugging
            CfnOutput(self, f"InstanceIdOutput{i+1}",
                      value=instance.instance_id,
                      description=f"Instance ID for MyInstance{i+1}")

# App setup
app = App()
vpc = ec2.Vpc(app, "MyVpc", max_azs=2)  # Create a VPC with 2 Availability Zones
TestAleph(app, "TestAleph", vpc=vpc)

app.synth()
