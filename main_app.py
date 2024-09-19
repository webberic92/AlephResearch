#SET ENV 
# set SECURITY_GROUP_ID=TEST_EXAMPLE
# set VPC_ID=TEST_EXAMPLE
# set AWS_ACCOUNT_ID=TESTEXAMPLE
# set AWS_REGION=TESTEXAMPLE
import os
from aws_cdk import App, Stack, Environment
from constructs import Construct
from aws_cdk import aws_ec2 as ec2
from infrastructure.security_groups import AlephOriginalSecurityGroupStack
from aleph_deployment.aleph_deployment_stack import testAleph

class MyCdkStack(Stack):
    def __init__(self, scope: Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Reference the existing VPC by looking it up
        existing_vpc = ec2.Vpc.from_lookup(self, "ExistingVPC", vpc_id=os.getenv("VPC_ID"))

        # Use the existing security group
        AlephOriginalSecurityGroupStack(self, "AlephSecurityGroupStack", 
                                         vpc=existing_vpc, 
                                         security_group_id=os.getenv("SECURITY_GROUP_ID"))

        # Run test
        testAleph(self, "deployAleph", vpc=existing_vpc)

app = App()

# Set environment for AWS account and region
env = Environment(account=os.getenv("AWS_ACCOUNT_ID"), region=os.getenv("AWS_REGION"))

MyCdkStack(app, "MyCdkStack", env=env)
app.synth()
