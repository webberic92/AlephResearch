from aws_cdk import (
    aws_ec2 as ec2,
    core
)

class MyEc2Stack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Define the EC2 instances
        for i in range(2):  # Adjust this for the number of instances you want to launch
            instance = ec2.Instance(self, f"MyInstance{i+1}",
                                    instance_type=ec2.InstanceType("t3.medium"),
                                    machine_image=ec2.MachineImage.latest_amazon_linux(),
                                    vpc=vpc,
                                    key_name="alephResearch"  # Replace with your key pair name
            )
            
            # User data for Aleph deployment
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git cargo jq",  # Install Git, Cargo, and jq
                "git clone https://github.com/webberic92/AlephResearch.git",  # Clone Aleph Protocol from GitHub
                "cd AlephResearch",  # Change directory to the cloned repo
                "git checkout aleph-orig",  # Checkout the aleph-orig branch
                "cargo build --release",  # Compile Aleph Protocol code
                
                # Generate keys for each node
                f"export NODE_INDEX={i+1} && ./target/release/generate_keys",  # Generate keys
                
                # Start node discovery in the background
                "nohup ./target/release/node_discovery > /home/aleph-node/node_discovery.log 2>&1 &", 
                
                # Wait for node discovery to produce output
                "while [ ! -f /home/aleph-node/known_nodes.json ]; do sleep 1; done",  # Wait for the discovery to write the known nodes to a file
                
                # Check if the node is the first one
                f"if [ {i+1} -eq 1 ]; then "
                "echo '[network]\\nlisten_address = \"/ip4/0.0.0.0/tcp/30333\"\\nbootnodes = []\\n' > /home/aleph-node/aleph-node-config.toml; "
                "else "
                "BOOTNODE_ADDRESSES=$(cat /home/aleph-node/known_nodes.json | jq -r '.[]' | tr '\\n' ' ' || echo ''); "
                "echo '[network]\\nlisten_address = \"/ip4/0.0.0.0/tcp/30333\"\\nbootnodes = [$BOOTNODE_ADDRESSES]\\n' > /home/aleph-node/aleph-node-config.toml; "
                "fi",
                
                # Additional configuration (if needed)
                "echo '[consensus]\\nbatch_size = 256\\ntimeout = 5000\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[data]\\ndata_dir = \"/home/aleph-node/data\"\\nlog_dir = \"/home/aleph-node/logs\"\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[logging]\\nlevel = \"info\"\\n' >> /home/aleph-node/aleph-node-config.toml",
                "echo '[metrics]\\nenabled = true\\n' >> /home/aleph-node/aleph-node-config.toml",
                
                # Run the Aleph node with the newly created config
                "./target/release/aleph-node --config /home/aleph-node/aleph-node-config.toml"
            )
            
            # Expose some outputs like instance IDs, IPs, etc., for debugging
            core.CfnOutput(self, f"InstanceIdOutput{i+1}",
                           value=instance.instance_id,
                           description=f"Instance ID for MyInstance{i+1}")
