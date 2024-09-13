# rds_stack.py
# RDS instance provisioning and schema creation for storing logs.

from aws_cdk import (
    aws_rds as rds,
    aws_ec2 as ec2,
    core
)

class RdsStack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, vpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # RDS PostgreSQL instance provisioning
        db_instance = rds.DatabaseInstance(self, "PostgresInstance",
            instance_type=ec2.InstanceType("t3.medium"),
            vpc=vpc,
            instance_class=rds.InstanceType.of(
                rds.InstanceClass.BURSTABLE2, rds.InstanceSize.MICRO
            ),
            database_name="aleph_logs",
            engine=rds.DatabaseInstanceEngine.postgres(version=rds.PostgresEngineVersion.VER_13_4),
            credentials=rds.Credentials.from_generated_secret("admin"),  # Change 'admin' to your preferred username
            removal_policy=core.RemovalPolicy.DESTROY,  # Ensure to change for production use
            deletion_protection=False,
            publicly_accessible=False
        )

        # Output RDS instance details
        core.CfnOutput(self, "RDSInstanceEndpoint",
                       value=db_instance.db_instance_endpoint_address,
                       description="The endpoint of the RDS PostgreSQL instance.")
        
        core.CfnOutput(self, "RDSInstancePort",
                       value=str(db_instance.db_instance_endpoint_port),
                       description="The port of the RDS PostgreSQL instance.")

        core.CfnOutput(self, "RDSInstanceUsername",
                       value="admin",  # Use the actual username if changed
                       description="The username for the RDS PostgreSQL instance.")

