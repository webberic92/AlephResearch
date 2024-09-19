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
                                    machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                    vpc=vpc,
                                    key_name="alephResearch" # Replace with your key pair name
            )
            
            # Install dependencies using user data script
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y rust cargo",
                "cargo install tokio" 
                # Add additional dependencies as needed
            )