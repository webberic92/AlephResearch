from aws_cdk import aws_ec2 as ec2
from constructs import Construct

class AlephOriginalSecurityGroupStack(Construct):
    def __init__(self, scope: Construct, id: str, vpc: ec2.Vpc, security_group_id: str) -> None:
        super().__init__(scope, id)

        # Import existing security group
        self.security_group = ec2.SecurityGroup.from_security_group_id(self, "ExistingSecurityGroup", security_group_id)

        # Now you can use self.security_group in your other resources
        

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
