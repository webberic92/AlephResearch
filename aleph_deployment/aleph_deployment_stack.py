# ec2_instances_stack.py
# EC2 instance provisioning and setup, including user data for dependencies.
from aws_cdk import (
    aws_ec2 as ec2,
    core
)

class MyEc2Stack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

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
                "sudo yum install -y git cargo jq python3",  # Install necessary packages
                "pip3 install psycopg2-binary",  # Install psycopg2 for PostgreSQL
                "git clone https://github.com/webberic92/AlephResearch.git",  # Clone Aleph Protocol repository
                "cd AlephResearch",  # Change directory to the cloned repo
                "git checkout aleph-orig",  # Checkout the aleph-orig branch
                "cargo build --release",  # Build Aleph Protocol code
                
                "mkdir -p /home/aleph-node",  # Create directory if it doesn't exist
                # Generate keys for each node
                f"export NODE_INDEX={i+1} && ./target/release/generate_keys",  # Generate keys
                
                # Start node discovery
                "nohup ./target/release/node_discovery > /home/aleph-node/node_discovery.log 2>&1 &", 
                
                # Wait for node discovery to complete
                "while [ ! -f /home/aleph-node/known_nodes.json ]; do sleep 1; done",
                
                # Dynamic IP allocation and configuration
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
                
                # Run the Aleph node
                "./target/release/aleph-node --config /home/aleph-node/aleph-node-config.toml",

                # Copy the log ingestion script
                "cp /home/aleph-node/AlephResearch/database_and_storage/log_ingestion_script.py /home/aleph-node/",
                
                # Periodically ingest logs to RDS
                "while true; do",
                "  python3 /home/aleph-node/log_ingestion_script.py",
                "  sleep 300  # Every 5 minutes",
                "done"
            )
            
            # Output instance details for debugging
            core.CfnOutput(self, f"InstanceIdOutput{i+1}",
                           value=instance.instance_id,
                           description=f"Instance ID for MyInstance{i+1}")
