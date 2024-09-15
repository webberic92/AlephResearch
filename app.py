# app.py or cdk_app.py
from aws_cdk import core
from database_and_storage.rds_stack import RdsStack
from infrastructure.vpc_stack import VpcStack

class MyCdkApp(core.App):
    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)

        # Create VPC stack
        vpc_stack = VpcStack(self, "VpcStack")

        # Create RDS stack
        RdsStack(self, "RdsStack", vpc=vpc_stack.vpc)

app = MyCdkApp()
