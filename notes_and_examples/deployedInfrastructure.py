# main_app.py
from aws_cdk import App
from constructs import Construct
from infrastructure.vpc_stack import AlephVPC
from database_and_storage.rds_stack import AlephRdsStack
from infrastructure.security_groups import AlephOriginalSecurityGroupStack  # Import security group stack

class MyCdkApp(App):
    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)

        # Create VPC stack
        vpc_stack = AlephVPC(self, "AlephVpcStack")

        # Create RDS stack
        AlephRdsStack(self, "AlephRdsStack", vpc=vpc_stack.vpc)

        # Create Security Group stack
        AlephOriginalSecurityGroupStack(self, "AlephSecurityGroupStack", vpc=vpc_stack.vpc)

app = MyCdkApp()
app.synth()
