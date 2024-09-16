from aws_cdk import (
    aws_rds as rds,
    aws_ec2 as ec2,
    Stack,
    CfnOutput,
    RemovalPolicy
)
from constructs import Construct

class AlephRdsStack(Stack):
    def __init__(self, scope: Construct, id: str, vpc: ec2.IVpc, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # RDS PostgreSQL instance provisioning
        db_instance = rds.DatabaseInstance(self, "PostgresInstance",
            instance_type=ec2.InstanceType("t3.micro"),
            vpc=vpc,
            database_name="aleph_orig_logs",
            engine=rds.DatabaseInstanceEngine.postgres(version=rds.PostgresEngineVersion.VER_14_13),
            credentials=rds.Credentials.from_generated_secret("dbadmin"),  # Update username here
            removal_policy=RemovalPolicy.DESTROY,
            deletion_protection=False,
            publicly_accessible=False
        )

        # Output RDS instance details
        CfnOutput(self, "RDSInstanceEndpoint",
                   value=db_instance.db_instance_endpoint_address,
                   description="The endpoint of the RDS PostgreSQL instance.")
        
        CfnOutput(self, "RDSInstancePort",
                   value=str(db_instance.db_instance_endpoint_port),
                   description="The port of the RDS PostgreSQL instance.")

        CfnOutput(self, "RDSInstanceUsername",
                   value="dbadmin",  # Update username here
                   description="The username for the RDS PostgreSQL instance.")

        # Output the secret ARN for the RDS credentials
        secret = db_instance.secret
        if secret:
            CfnOutput(self, "RDSInstanceSecretArn",
                       value=secret.secret_arn,
                       description="The ARN of the RDS credentials secret.")
        else:
            print("Warning: RDS credentials secret was not created.")
