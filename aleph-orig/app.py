# Main entry point for the CDK application, initializes all stacks.
#!/usr/bin/env python3

# Notes:

#     Import Paths: Ensure the import paths match your directory structure. You might need to adjust these paths if your modules or stack files are located differently.
#     Stack Initialization: Each stack is initialized with a unique identifier (e.g., "CiCdPipelineStack", "AlephDeploymentStack"). Modify these names as needed to fit your naming conventions.
#     Execution: Run this script to deploy your CDK application. Ensure you have the AWS CDK CLI installed and configured.

# What’s Missing/Needed from You:

#     Stack Classes: Confirm that the stack classes (e.g., CiCdPipelineStack, AlephDeploymentStack) are correctly defined and imported in your respective modules.
#     Dependencies: Make sure all necessary CDK libraries and dependencies are included in your requirements.txt or package.json, depending on your setup.

from aws_cdk import core
from ci_cd.ci_cd_pipeline import CiCdPipelineStack
from aleph_deployment.aleph_deployment_stack import AlephDeploymentStack
from monitoring_and_logging.monitoring_stack import MonitoringStack
from database_and_storage.rds_stack import RdsStack
from shutdown.shutdown_scheduler import ShutdownSchedulerStack
from testing_and_analysis.test_runner import TestRunnerStack

class MyCdkApp(core.App):
    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)

        # Initialize stacks
        CiCdPipelineStack(self, "CiCdPipelineStack")
        AlephDeploymentStack(self, "AlephDeploymentStack")
        MonitoringStack(self, "MonitoringStack")
        RdsStack(self, "RdsStack")
        ShutdownSchedulerStack(self, "ShutdownSchedulerStack")
        TestRunnerStack(self, "TestRunnerStack")

if __name__ == "__main__":
    app = MyCdkApp()
    app.synth()
