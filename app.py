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
    def __init__(self, scope: Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Create a VPC
        vpc = ec2.Vpc(self, "AlephVpc",
                      max_azs=2,  # Set the number of Availability Zones
                      nat_gateways=1)  # Set number of NAT gateways

        # Test number for instances
        INSTANCES_NUMBER = 4
        unique_id = datetime.now().strftime("%Y%m%d%H%M")

        # Create a log group for CloudWatch logging
        log_group = logs.LogGroup(self, "AlephNodeLogGroup",
                                  log_group_name=f"/aleph-research/nodes-{unique_id}")

        # Define an IAM role for the EC2 instances
        role = iam.Role(self, "InstanceRole",
                        assumed_by=iam.ServicePrincipal("ec2.amazonaws.com"),
                        managed_policies=[
                            iam.ManagedPolicy.from_aws_managed_policy_name("AmazonSSMManagedInstanceCore"),
                            iam.ManagedPolicy.from_aws_managed_policy_name("CloudWatchAgentServerPolicy"),
                        ])

        # Define the EC2 instances
        for i in range(INSTANCES_NUMBER):
            instance = ec2.Instance(self, f"MyInstance{i + 1}",
                                    instance_type=ec2.InstanceType("t3.medium"),
                                    machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                    vpc=vpc,
                                    key_name="alephResearch",  # Make sure this key pair exists
                                    role=role)  # Attach the IAM role

            # Output instance details for debugging
            CfnOutput(self, f"InstanceIdOutput{i + 1}-{unique_id}",
                      value=instance.instance_id,
                      description=f"Instance ID for MyInstance{i + 1}")

            # Install dependencies and configure user data
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git cargo jq python3 awslogs amazon-ssm-agent",
                "git clone https://github.com/webberic92/AlephResearch.git",
                "cd AlephResearch",
                "git checkout aleph-orig",
                "cargo build --release",
                "mkdir -p /home/aleph-node/logs/{memory_usage, node_status, error_logs, transaction_metrics, network_metrics}",
                "mkdir -p /home/aleph-node",

                f"export NODE_INDEX={i + 1} && ./target/release/generate_keys",
                "nohup ./target/release/node_discovery > /home/aleph-node/node_discovery.log 2>&1 &",
                "while [ ! -f /home/aleph-node/known_nodes.json ]; do sleep 1; done",

                "KNOWN_NODES_IPS=$(jq -r '.[] | .ip_address' /home/aleph-node/known_nodes.json)",
                "if [ -z \"$KNOWN_NODES_IPS\" ]; then",
                "   NEW_IP=\"192.168.0.1\";",
                "else",
                "   LARGEST_IP=$(echo $KNOWN_NODES_IPS | tr ' ' '\\n' | sort -t '.' -k 4,4n | tail -1);",
                "   LAST_OCTET=$(echo $LARGEST_IP | awk -F '.' '{print $4}');",
                "   NEW_LAST_OCTET=$((LAST_OCTET + 1));",
                "   NEW_IP=$(echo $LARGEST_IP | sed 's/\\.[0-9]*$/.'$NEW_LAST_OCTET'/');",
                "fi",

                "echo '[network]' > /home/aleph-node/aleph-node-config.toml",
                "echo 'listen_address = \"/ip4/${NEW_IP}/tcp/30333\"' >> /home/aleph-node/aleph-node-config.toml",
                "if [ ! -z \"$KNOWN_NODES_IPS\" ]; then",
                "   BOOTNODE_ADDRESSES=$(cat /home/aleph-node/known_nodes.json | jq -r '.[] | .multiaddr' | tr '\\n' ' ');",
                "   echo 'bootnodes = [$BOOTNODE_ADDRESSES]' >> /home/aleph-node/aleph-node-config.toml;",
                "else",
                "   echo 'bootnodes = []' >> /home/aleph-node/aleph-node-config.toml;",
                "fi",

                "echo '[consensus]\\nbatch_size = 256\\ntimeout = 5000\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[data]\\ndata_dir = \"/home/aleph-node/data\"\\nlog_dir = \"/home/aleph-node/logs\"\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[logging]\\nlevel = \"info\"\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[metrics]\\nenabled = true\\n' >> /home/aleph-node/aleph-node-config.toml",
                "./target/release/aleph-node --config /home/aleph-node/aleph-node-config.toml",

                "sudo tee /etc/awslogs/awslogs.conf << EOF",
                "[general]",
                "state_file = /var/lib/awslogs/agent-state",
                "[/home/aleph-node/logs]",
                "file = /home/aleph-node/logs/*.log",
                f"log_group_name = {log_group.log_group_name}",
                f"log_stream_name = MyInstance{i + 1}/aleph-node-log",
                "datetime_format = %Y-%m-%d %H:%M:%S",
                "EOF",

                "sudo systemctl start awslogsd",
                "while true; do",
                "  echo \"$(date) - Node $NODE_INDEX running with IP $NEW_IP\" >> /home/aleph-node/logs/node_status/node_status.log",
                "  echo \"$(date) - Memory Usage: $(free -m | awk '/Mem:/ {print $3}') MB\" >> /home/aleph-node/logs/memory_usage/memory_usage.log",
                "  sleep 300",
                "done &",
                "trap 'echo \"$(date) - Error occurred: $?\" >> /home/aleph-node/logs/error_logs/error.log; exit' ERR",
                "echo \"$(date) - Transaction Metrics Placeholder\" >> /home/aleph-node/logs/transaction_metrics/transaction_metrics.log",
                "echo \"$(date) - Network Metrics Placeholder\" >> /home/aleph-node/logs/network_metrics/network_metrics.log",
                "while true; do",
                "  if [ $(jq length /home/aleph-node/known_nodes.json) -eq {INSTANCES_NUMBER} ]; then",
                "    echo 'All nodes discovered. Sending termination signal.';",
                "    aws ec2 terminate-instances --instance-ids $(curl -s http://169.254.169.254/latest/meta-data/instance-id) --region us-east-1;",
                "    exit 0;",
                "  fi;",
                "  sleep 10;",
                "done"
            )

def main():
    app = App()
    TestAleph(app, "TestAlephStack", env=Environment(account="532911864509", region="us-east-1"))
    app.synth()

if __name__ == "__main__":
    main()
