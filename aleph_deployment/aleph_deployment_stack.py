# ec2_instances_stack.py
# EC2 instance provisioning and setup, including user data for dependencies.
from aws_cdk import (
    aws_ec2 as ec2,
    aws_logs as logs,
    core
)

class MyEc2Stack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Create a log group for CloudWatch logging
        log_group = logs.LogGroup(self, "AlephNodeLogGroup", 
                                  log_group_name="/aleph-research/nodes",
                                  retention=logs.RetentionDays.ONE_WEEK)  # Logs will be kept for one week

        # Define the EC2 instances
        for i in range(2):  # Launch two instances
            instance = ec2.Instance(self, f"MyInstance{i+1}",
                                    instance_type=ec2.InstanceType("t3.medium"),
                                    machine_image=ec2.MachineImage.latest_amazon_linux(),
                                    vpc=vpc,
                                    key_name="alephResearch"  # Replace with your key pair name
            )
            
            # Install dependencies and configure user data
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git cargo jq python3 awslogs",  # Install necessary packages including AWS CloudWatch agent
                "git clone https://github.com/webberic92/AlephResearch.git",  # Clone Aleph Protocol repository
                "cd AlephResearch",  # Change directory to the cloned repo
                "git checkout aleph-orig",  # Checkout the aleph-orig branch
                "cargo build --release",  # Build Aleph Protocol code
                
                "mkdir -p /home/aleph-node/logs/{memory_usage, node_status, error_logs, transaction_metrics, network_metrics}",  # Create log directories
                "mkdir -p /home/aleph-node",  # Create main directory if it doesn't exist
                
                # Generate keys for each node
                f"export NODE_INDEX={i+1} && ./target/release/generate_keys",  # Generate keys
                
                # Start node discovery in the background, directing output to the log file
                "nohup ./target/release/node_discovery > /home/aleph-node/node_discovery.log 2>&1 &",

                # Wait for node discovery to complete
                "while [ ! -f /home/aleph-node/known_nodes.json ]; do sleep 1; done",

                # Extract known node IPs from the discovery file
                "KNOWN_NODES_IPS=$(jq -r '.[] | .ip_address' /home/aleph-node/known_nodes.json)",
                "if [ -z \"$KNOWN_NODES_IPS\" ]; then",
                "   NEW_IP=\"192.168.0.1\";",  # Default IP if no nodes are known
                "else",
                "   LARGEST_IP=$(echo $KNOWN_NODES_IPS | tr ' ' '\\n' | sort -t '.' -k 4,4n | tail -1);",
                "   LAST_OCTET=$(echo $LARGEST_IP | awk -F '.' '{print $4}');",
                "   NEW_LAST_OCTET=$((LAST_OCTET + 1));",
                "   NEW_IP=$(echo $LARGEST_IP | sed 's/\\.[0-9]*$/.'$NEW_LAST_OCTET'/');",
                "fi",

                # Write the network configuration to a TOML config file
                "echo '[network]' > /home/aleph-node/aleph-node-config.toml",
                "echo 'listen_address = \"/ip4/${NEW_IP}/tcp/30333\"' >> /home/aleph-node/aleph-node-config.toml",

                # Set bootnodes in the configuration
                "if [ ! -z \"$KNOWN_NODES_IPS\" ]; then",
                "   BOOTNODE_ADDRESSES=$(cat /home/aleph-node/known_nodes.json | jq -r '.[] | .multiaddr' | tr '\\n' ' ');",
                "   echo 'bootnodes = [$BOOTNODE_ADDRESSES]' >> /home/aleph-node/aleph-node-config.toml;",
                "else",
                "   echo 'bootnodes = []' >> /home/aleph-node/aleph-node-config.toml;",
                "fi",

                # Add consensus configuration
                "echo '[consensus]\\nbatch_size = 256\\ntimeout = 5000\\n' >> /home/aleph-node/aleph-node-config.toml",

                # Add data directories for logs and persistent data
                "echo '[data]\\ndata_dir = \"/home/aleph-node/data\"\\nlog_dir = \"/home/aleph-node/logs\"\\n' >> /home/aleph-node/aleph-node-config.toml",

                # Set logging level to 'info'
                "echo '[logging]\\nlevel = \"info\"\\n' >> /home/aleph-node/aleph-node-config.toml",

                # Enable metrics collection
                "echo '[metrics]\\nenabled = true\\n' >> /home/aleph-node/aleph-node-config.toml",

                # Start the Aleph node with the generated configuration file
                "./target/release/aleph-node --config /home/aleph-node/aleph-node-config.toml",

                # Logging setup for CloudWatch
                "sudo tee /etc/awslogs/awslogs.conf << EOF",
                "[general]",
                "state_file = /var/lib/awslogs/agent-state",
                
                "[/home/aleph-node/logs]",
                "file = /home/aleph-node/logs/*.log",
                f"log_group_name = {log_group.log_group_name}",
                f"log_stream_name = MyInstance{i+1}/aleph-node-log",
                "datetime_format = %Y-%m-%d %H:%M:%S",
                "EOF",
                
                "sudo systemctl start awslogsd",  # Start AWS CloudWatch Logs agent

                # Log system details, including memory usage and node status
                "while true; do",
                "  echo \"$(date) - Node $NODE_INDEX running with IP $NEW_IP\" >> /home/aleph-node/logs/node_status/node_status.log",
                "  echo \"$(date) - Memory Usage: $(free -m | awk '/Mem:/ {print $3}') MB\" >> /home/aleph-node/logs/memory_usage/memory_usage.log",
                "  sleep 300  # Every 5 minutes",
                "done",

                # Log error messages
                "trap 'echo \"$(date) - Error occurred: $?\" >> /home/aleph-node/logs/error_logs/error.log; exit' ERR",
                
                # Placeholder for transaction metrics logging
                "echo \"$(date) - Transaction Metrics Placeholder\" >> /home/aleph-node/logs/transaction_metrics/transaction_metrics.log",
                
                # Placeholder for network metrics logging
                "echo \"$(date) - Network Metrics Placeholder\" >> /home/aleph-node/logs/network_metrics/network_metrics.log"
            )
            
            # Output instance details for debugging
            core.CfnOutput(self, f"InstanceIdOutput{i+1}",
                           value=instance.instance_id,
                           description=f"Instance ID for MyInstance{i+1}")
