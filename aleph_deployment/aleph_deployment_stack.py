from aws_cdk import (
    aws_ec2 as ec2,
    aws_logs as logs,
    Stack,
    CfnOutput
)
from constructs import Construct
from datetime import datetime
class testAleph(Stack):
    def __init__(self, scope: Construct, id: str, vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)
        
        # Test number for instances
        INSTANCES_NUMBER = 4
        # Create a log group for CloudWatch logging
        unique_id = datetime.now().strftime("%Y%m%d%H%M")

        log_group = logs.LogGroup(self, "AlephNodeLogGroup", log_group_name=f"/aleph-research/nodes-{unique_id}")

        # Define the EC2 instances
        for i in range(INSTANCES_NUMBER):
            instance = ec2.Instance(self, f"MyInstance{i+1}",
                                    instance_type=ec2.InstanceType("t3.medium"),
                                    machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                    vpc=vpc,
                                    key_name="alephResearch"
            )
            
            # Install dependencies and configure user data
            instance.user_data.add_commands(
                "sudo yum update -y",
                "sudo yum install -y git cargo jq python3 awslogs amazon-ssm-agent",
                "git clone https://github.com/webberic92/AlephResearch.git",
                "cd AlephResearch",
                "git checkout aleph-orig",
                "cargo build --release",
                # ... [rest of your user data commands] ...
            )
            
            # Output instance details for debugging
            CfnOutput(self, f"InstanceIdOutput{i+1}",
                      value=instance.instance_id,
                      description=f"Instance ID for MyInstance{i+1}")
