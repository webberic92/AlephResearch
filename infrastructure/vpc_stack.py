from aws_cdk import (
    aws_ec2 as ec2,
    Stack
)
from constructs import Construct

class AlephVPC(Stack):
    def __init__(self, scope: Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Create a VPC with public and private subnets
        self.vpc = ec2.Vpc(self, "AlephVPC",
                          max_azs=3,
                          nat_gateways=1,
                          subnet_configuration=[
                              ec2.SubnetConfiguration(
                                  name="Public",
                                  subnet_type=ec2.SubnetType.PUBLIC,
                                  cidr_mask=24
                              ),
                              ec2.SubnetConfiguration(
                                  name="Private",
                                  subnet_type=ec2.SubnetType.PRIVATE_WITH_EGRESS,
                                  cidr_mask=24
                              )
                          ]
        )
