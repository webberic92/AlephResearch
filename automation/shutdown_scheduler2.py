# shutdown_scheduler2.py
#Option 2: Using CloudWatch Events Directly

from aws_cdk import (
    aws_events as events,
    aws_events_targets as targets,
    aws_lambda as lambda_,
    core
)

class ShutdownSchedulerStack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Define Lambda function for shutdown
        shutdown_lambda = lambda_.Function(self, "ShutdownFunction",
            runtime=lambda_.Runtime.PYTHON_3_8,
            handler="shutdown_script.lambda_handler",
            code=lambda_.Code.from_asset("path/to/lambda/code"),
            environment={
                "INSTANCE_IDS": "i-1234567890abcdef0,i-abcdef1234567890"  # Set instance IDs here or dynamically
            }
        )

        # Create CloudWatch Event rule to trigger Lambda function
        rule = events.Rule(self, "ShutdownRule",
            schedule=events.Schedule.cron(minute="0", hour="0"),  # Example: Schedule to run daily at midnight
            targets=[targets.LambdaFunction(handler=shutdown_lambda)]
        )

        core.CfnOutput(self, "SchedulerRuleArn",
                       value=rule.rule_arn,
                       description="The ARN of the CloudWatch Events rule for triggering the shutdown Lambda function.")
