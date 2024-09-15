from aws_cdk import App
from constructs import Construct
from infrastructure.vpc_stack import MyVpcStack
from database_and_storage.rds_stack import RdsStack

class MyCdkApp(App):
    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)

        # Create VPC stack
        vpc_stack = MyVpcStack(self, "VpcStack")

        # Create RDS stack
        RdsStack(self, "RdsStack", vpc=vpc_stack.vpc)

app = MyCdkApp()
app.synth()
