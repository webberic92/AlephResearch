# Setup for AWS Systems Manager or CloudWatch Events to trigger shutdown script.
# shutdown_scheduler.py

from aws_cdk import (
    aws_ssm as ssm,
    aws_events as events,
    aws_events_targets as targets,
    core
)

class ShutdownSchedulerStack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Create SSM Document for shutdown script
        document = ssm.CfnDocument(self, "ShutdownDocument",
            content={
                "schemaVersion": "2.2",
                "description": "Stop EC2 instances after tests",
                "mainSteps": [
                    {
                        "action": "aws:runCommand",
                        "name": "stopInstances",
                        "inputs": {
                            "DocumentName": "AWS-RunShellScript",
                            "Parameters": {
                                "commands": [
                                    "python3 /path/to/shutdown_script.py"
                                ]
                            }
                        }
                    }
                ]
            },
            document_type="Command"
        )

        # Create CloudWatch Event rule to schedule the shutdown
        rule = events.Rule(self, "ShutdownRule",
            schedule=events.Schedule.cron(minute="0", hour="0"),  # Example: Schedule to run daily at midnight
            targets=[targets.SsmDocument(document=document)]
        )

        core.CfnOutput(self, "SchedulerRuleArn",
                       value=rule.rule_arn,
                       description="The ARN of the CloudWatch Events rule for triggering the shutdown script.")
