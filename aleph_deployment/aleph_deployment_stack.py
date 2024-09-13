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
                                    key_name="your-key-pair-name"  # Replace with your key pair name
            )
            
            # User data for Aleph deployment
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git cargo",  # Install Git and Cargo
                "git clone https://github.com/webberic92/AlephResearch.git",  # Clone Aleph Protocol from GitHub
                "cd AlephResearch",  # Change directory to the cloned repo
                "git checkout aleph-orig",  # Checkout the aleph-orig branch
                "cargo build --release",  # Compile Aleph Protocol code
                
                # Generate keys for each node
                f"export NODE_INDEX={i+1} && ./target/release/generate_keys",  # Generate keys
                
                # Create a configuration file for each node
                f"echo '[node]\\nname = \"aleph-node-{i+1}\"\\nidentity = \"/home/aleph-node/identity-{i+1}.pem\"\\n' > aleph-node-config.toml",
                "echo '[network]\\nlisten_address = \"/ip4/0.0.0.0/tcp/30333\"\\nbootnodes = [\"/ip4/127.0.0.1/tcp/30333/p2p/your_bootnode_id\"]\\n' >> aleph-node-config.toml",
                "echo '[consensus]\\nbatch_size = 256\\ntimeout = 5000\\n' >> aleph-node-config.toml",
                "echo '[data]\\ndata_dir = \"/home/aleph-node/data\"\\nlog_dir = \"/home/aleph-node/logs\"\\n' >> aleph-node-config.toml",
                "echo '[logging]\\nlevel = \"info\"\\n' >> aleph-node-config.toml",
                "echo '[metrics]\\nenabled = true\\n' >> aleph-node-config.toml",
                
                # Run the Aleph node with the newly created config
                f"./target/release/aleph-node --config aleph-node-config.toml"
            )
            
            # Expose some outputs like instance IDs, IPs, etc., for debugging
            core.CfnOutput(self, f"InstanceIdOutput{i+1}",
                           value=instance.instance_id,
                           description=f"Instance ID for MyInstance{i+1}")
