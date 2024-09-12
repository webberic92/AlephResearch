# aleph_deployment_stack.py
# Deployment of Aleph protocol on EC2 instances, GitHub cloning, compiling, and configuration.
from aws_cdk import (
    aws_ec2 as ec2,
    core
)
import os

class AlephDeploymentStack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, vpc, security_group, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Replace with your GitHub repository URL
        github_repo_url = "https://github.com/webberic92/AlephResearch.git"  # Ask user to confirm or provide

        # Instance configuration
        for i in range(2):  # Assuming two instances for now
            instance = ec2.Instance(self, f"AlephInstance{i+1}",
                                    instance_type=ec2.InstanceType("t3.medium"),
                                    machine_image=ec2.MachineImage.latest_amazon_linux(),
                                    vpc=vpc,
                                    security_group=security_group,
                                    key_name="your-key-pair-name"  # Ask user to provide key pair name
            )

            # User data for Aleph deployment
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git",  # Install Git
                f"git clone {github_repo_url}",  # Clone Aleph Protocol from GitHub
                "cd AlephResearch",  # Change directory to the cloned repo
                "sudo yum install -y cargo",  # Install Rust (cargo) and other dependencies as needed
                "cargo build --release",  # Compile Aleph Protocol code
                "export ALEPH_CONFIG=your_config_file",  # Ask user to provide or confirm configuration file details
                "./target/release/aleph-node --config $ALEPH_CONFIG"  # Run the Aleph node with the provided config
            )
            
            # Expose some outputs like instance IDs, IPs, etc., for debugging
            core.CfnOutput(self, f"InstanceIdOutput{i+1}",
                           value=instance.instance_id,
                           description=f"Instance ID for AlephInstance{i+1}")