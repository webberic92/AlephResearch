# security_groups.py
# Security group definitions and rules for EC2 instances.
from aws_cdk import (
    aws_ec2 as ec2,
    Stack
)
from constructs import Construct

class AlephOriginalSecurityGroupStack(Stack):
    def __init__(self, scope: Construct, id: str, vpc: ec2.IVpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Define a security group
        self.security_group = ec2.SecurityGroup(self, "AlephOriginalSecurityGroup",
                                           vpc=vpc,
                                           description="Allow SSH and application communication for the original ALEPH nodes.",
                                           allow_all_outbound=True
        )

        # Allow SSH access (port 22)
        self.security_group.add_ingress_rule(
            peer=ec2.Peer.any_ipv4(),
            connection=ec2.Port.tcp(22),
            description="Allow SSH access"
        )

        # Allow application traffic (port 8080)
        self.security_group.add_ingress_rule(
            peer=ec2.Peer.any_ipv4(),
            connection=ec2.Port.tcp(8080),
            description="Allow application traffic"
        )
