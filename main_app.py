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
        print("VPC_ID:", os.getenv("VPC_ID"))
        print("SECURITY_GROUP_ID:", os.getenv("SECURITY_GROUP_ID"))
        print("AWS_ACCOUNT_ID:", os.getenv("AWS_ACCOUNT_ID"))
        print("AWS_REGION:", os.getenv("AWS_REGION"))

    
        # Reference the existing VPC by looking it up
        existing_vpc = ec2.Vpc.from_lookup(self, "AlephVPC", vpc_id=os.getenv("VPC_ID"))

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


    #    # Initialize a session using the default profile (or set your own AWS access keys if needed)
    #     session = boto3.Session(
    #         aws_access_key_id=os.getenv("AWS_ACCESS_KEY_ID"),  # Optional
    #         aws_secret_access_key=os.getenv("AWS_SECRET_ACCESS_KEY"),  # Optional
    #         region_name=os.getenv("AWS_REGION")
    #     )
