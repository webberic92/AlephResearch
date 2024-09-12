# monitoring_stack.py
# Setup for CloudWatch, Prometheus Node Exporter, or other monitoring tools.

from aws_cdk import (
    aws_ec2 as ec2,
    aws_cloudwatch as cloudwatch,
    core
)

class MonitoringStack(core.Stack):
    def __init__(self, scope: core.Construct, id: str, instances, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Install and configure CloudWatch Agent on each instance
        for i, instance in enumerate(instances):
            instance.user_data.add_commands(
                "sudo yum install -y amazon-cloudwatch-agent",  # Install CloudWatch Agent
                "sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-config-wizard",  # Launch configuration wizard
                "sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a start"  # Start CloudWatch Agent
            )

            # Install and configure Prometheus Node Exporter on each instance
            instance.user_data.add_commands(
                "wget https://github.com/prometheus/node_exporter/releases/download/v1.3.1/node_exporter-1.3.1.linux-amd64.tar.gz",
                "tar -xvf node_exporter-1.3.1.linux-amd64.tar.gz",
                "sudo mv node_exporter-1.3.1.linux-amd64/node_exporter /usr/local/bin/",
                "sudo useradd --no-create-home node_exporter",
                "sudo bash -c 'echo \"[Unit]\nDescription=Prometheus Node Exporter\n\n[Service]\nUser=node_exporter\nExecStart=/usr/local/bin/node_exporter\n\n[Install]\nWantedBy=default.target\" > /etc/systemd/system/node_exporter.service'",
                "sudo systemctl daemon-reload",
                "sudo systemctl start node_exporter",
                "sudo systemctl enable node_exporter"
            )

            # Create CloudWatch Dashboards and Alarms
            cpu_metric = cloudwatch.Metric(
                namespace="AWS/EC2",
                metric_name="CPUUtilization",
                dimensions={"InstanceId": instance.instance_id}
            )

            cpu_alarm = cloudwatch.Alarm(self, f"CpuAlarm{i+1}",
                                         metric=cpu_metric,
                                         threshold=80,
                                         evaluation_periods=1,
                                         alarm_actions=[],  # Ask user to provide SNS Topic for alarm actions
                                         alarm_description="Alarm if CPU utilization exceeds 80%")
            
            core.CfnOutput(self, f"CloudWatchDashboardOutput{i+1}",
                           value=f"https://console.aws.amazon.com/cloudwatch/home?region={self.region}#dashboards:name=AlephDashboard",
                           description=f"CloudWatch Dashboard for AlephInstance{i+1}")

