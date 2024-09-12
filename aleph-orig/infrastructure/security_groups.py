# security_groups.py
# Security group definitions and rules for EC2 instances.
from aws_cdk import (
    aws_ec2 as ec2,
    core
)

class MySecurityGroupStack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Define a security group
        self.security_group = ec2.SecurityGroup(self, "MySecurityGroup",
                                           vpc=vpc,
                                           description="Allow SSH and application communication",
                                           allow_all_outbound=True
        )

        # Allow SSH access
        self.security_group.add_ingress_rule(
            peer=ec2.Peer.any_ipv4(),
            connection=ec2.Port.tcp(22),
            description="Allow SSH access"
        )

        # Allow application traffic (e.g., port 8080)
        self.security_group.add_ingress_rule(
            peer=ec2.Peer.any_ipv4(),
            connection=ec2.Port.tcp(8080),
            description="Allow application traffic"
        )