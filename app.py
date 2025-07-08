from aws_cdk import (
    aws_ec2 as ec2,
    aws_logs as logs,
    aws_iam as iam,
    Stack,
    CfnOutput,
    App,
)
from constructs import Construct
from datetime import datetime

class TestAleph(Stack):
    def __init__(self, scope: Construct, id: str, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)
        #MERKLE
        INSTANCE_TYPE ="c5n.xlarge" # Define the instance type
        INSTANCES_NUMBER = 5 # Define the number of instances
        BATCH_SIZE =  1 #(TX PER BATCH) Define the number of transactions in a batch
        TRANSACTION_SIZE = 256 #Bytes how many bytes per transaction
        SHARD_SIZE = max(1, min(BATCH_SIZE, INSTANCES_NUMBER - INSTANCES_NUMBER // 3))
        TOTAL_ROUNDS = 1 # Define the number of rounds
        unique_id = datetime.now().strftime("%Y%m%d%H%M")

        # Create a VPC within the scope of this Stack
        vpc = ec2.Vpc(self, "MyVpc", max_azs=2)

        # Create a log group for CloudWatch logging
        log_group = logs.LogGroup(self, "AlephNodeLogGroup", log_group_name=f"/aleph-research/nodes-{unique_id}")

        # Create a security group for EC2 instances with intra-VPC communication
        security_group = ec2.SecurityGroup(
            self, "AlephNodeSG",
            vpc=vpc,
            allow_all_outbound=True
        )
        security_group.add_ingress_rule(ec2.Peer.ipv4(vpc.vpc_cidr_block), ec2.Port.all_traffic(), "Allow VPC-wide communication")

        # IAM Role for EC2 Instances (for S3, CloudWatch, and SSM)
        instance_role = iam.Role(
            self, "InstanceRole",
            assumed_by=iam.ServicePrincipal("ec2.amazonaws.com")
        )
        # Attach policies for S3, CloudWatch Logs, and SSM Session Manager
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("CloudWatchLogsFullAccess"))
        # instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("AmazonS3ReadOnlyAccess"))
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("AmazonS3FullAccess"))
        instance_role.add_managed_policy(iam.ManagedPolicy.from_aws_managed_policy_name("AmazonSSMManagedInstanceCore"))

        # Define a lightweight t2.micro instance as the IP Manager
        ip_manager_instance = ec2.Instance(self, "IPManager",
                                           instance_type=ec2.InstanceType("t2.micro"),
                                           machine_image=ec2.MachineImage.latest_amazon_linux2(),
                                           vpc=vpc,
                                           security_group=security_group,
                                           role=instance_role
        )
        # Start the IP server for readiness tracking
        ip_manager_instance.user_data.add_commands(
            "sudo yum update -y",
            "sudo yum install -y python3 jq",
            "aws s3 cp s3://aleph-research/IpServer.py /home/ec2-user/ --quiet",
            "sudo chmod -R 777 /home/ec2-user",
            "cd /home/ec2-user", 
            f"python3 IpServer.py {INSTANCES_NUMBER} &"
        )

        for i in range(INSTANCES_NUMBER):
            ec2_instance = ec2.Instance(self, f"MyInstance{i+1}",
                instance_type=ec2.InstanceType(INSTANCE_TYPE),
                machine_image=ec2.MachineImage.latest_amazon_linux2(),
                vpc=vpc,
                security_group=security_group,
                role=instance_role,
                # credit_specification=ec2.CpuCredits.UNLIMITED,
            )

            ec2_instance.user_data.add_commands(
                # System updates and tools
                "sudo yum update -y",
                "sudo yum install -y gcc wget tar make bison git jq python3 awslogs amazon-ssm-agent aws-cli",

                # Increase max number of incoming connections
                "echo 'net.core.somaxconn=65535' >> /etc/sysctl.conf",

                # Increase maximum number of packets allowed to queue when interface receives them faster than kernel can process
                "echo 'net.core.netdev_max_backlog=16384' >> /etc/sysctl.conf",

                # Max number of connections that can be queued for acceptance
                "echo 'net.ipv4.tcp_max_syn_backlog=65535' >> /etc/sysctl.conf",

                # Reduce time TCP waits before closing sockets (helps with many short-lived connections)
                "echo 'net.ipv4.tcp_fin_timeout=15' >> /etc/sysctl.conf",

                # Allow reuse of sockets in TIME_WAIT (lowers connection overhead)
                "echo 'net.ipv4.tcp_tw_reuse=1' >> /etc/sysctl.conf",

                # Frequency of TCP keepalive messages (helps detect dead peers faster)
                "echo 'net.ipv4.tcp_keepalive_time=120' >> /etc/sysctl.conf",

                # Allow larger port range to reduce bind failures under many outbound connections
                "echo 'net.ipv4.ip_local_port_range=1024 65535' >> /etc/sysctl.conf",

                # Raise system-wide file descriptor cap (needed for many open connections/files)
                "echo 'fs.file-max=2097152' >> /etc/sysctl.conf",

                # Apply the sysctl changes immediately
                "sysctl -p",

                # Set soft limit for number of open file descriptors per user
                "echo '* soft nofile 1048576' | tee -a /etc/security/limits.conf",

                # Set hard limit for number of open file descriptors per user
                "echo '* hard nofile 1048576' | tee -a /etc/security/limits.conf",

                # Ensure root user also gets higher limits
                "echo 'root soft nofile 1048576' | tee -a /etc/security/limits.conf",
                "echo 'root hard nofile 1048576' | tee -a /etc/security/limits.conf",

                # Enable PAM module to enforce limits (required for limits.conf to take effect)
                "grep -qxF 'session required pam_limits.so' /etc/pam.d/common-session || echo 'session required pam_limits.so' | tee -a /etc/pam.d/common-session",
                "grep -qxF 'session required pam_limits.so' /etc/pam.d/su || echo 'session required pam_limits.so' | tee -a /etc/pam.d/su",

                # Raise ulimit for open files (applies immediately to shell, useful for logging/tests)
                "ulimit -n 1048576",
                # Aleph setup
                "mkdir -p /aleph/logs/",
                "touch /aleph/logs/node_status /aleph/logs/cpu_usage /aleph/logs/mem_usage",
                "chmod -R 777 /aleph/logs/",
                "echo 'Starting cpu and memory logs' >> /aleph/logs/node_status",
                "nohup sar -u 1 >> /aleph/logs/cpu_usage 2>&1 &",
                "nohup sar -r 1 >> /aleph/logs/mem_usage 2>&1 &",
                "aws s3 cp s3://aleph-research/aleph_rbc /aleph/ --quiet",
                "sudo chmod -R 777 /aleph/",

                # Capture IP and register
                "PRIVATE_IP=$(curl -s http://169.254.169.254/latest/meta-data/local-ipv4)",
                "echo \"PRIVATE_IP = $PRIVATE_IP\" >> /aleph/logs/node_status",

                f"""
                while true; do
                    RESPONSE=$(curl -s -o /dev/null -w "%{{http_code}}" -X POST -H 'Content-Type: application/json' -d '{{"node_ip": "'$PRIVATE_IP'"}}' http://{ip_manager_instance.instance_private_ip}:8080/node_ready)
                    if [ "$RESPONSE" -eq 200 ]; then
                        echo "Node registration success." >> /aleph/logs/node_status
                        break
                    else
                        echo "Node registration failed with status $RESPONSE. Retrying..." >> /aleph/logs/node_status
                        sleep 5
                    fi
                done
                """,

                # Wait for IP manager to report readiness
                f"""
                while true; do
                    if curl -s http://{ip_manager_instance.instance_private_ip}:8080/check_all_ready | grep -q '"all_ready": true'; then
                        echo 'IP Manager is reachable and all nodes are ready.' >> /aleph/logs/node_status
                        break
                    else
                        echo 'IP Manager not ready, retrying...' >> /aleph/logs/node_status
                    fi
                    sleep 5
                done
                """,
            )


            ec2_instance.user_data.add_commands(
                # Extract the private IP from logs
                "PRIVATE_IP=$(grep 'PRIVATE_IP =' /aleph/logs/node_status | awk -F '= ' '{print $2}')",

                # Retrieve all node IPs, exclude the current node's IP, and format them properly for the TOML configuration
                f"NODES=$(curl -s http://{ip_manager_instance.instance_private_ip}:8080/get_all_nodes | jq -r --arg PRIVATE_IP \"$PRIVATE_IP\" '.node_ips | map(select(. != $PRIVATE_IP)) | map(\"\\\"\" + . + \":30333\\\"\") | join(\", \")')",
                # Log the filtered node list for verification
                "echo \"Retrieved all nodes for nodes (excluding self): $NODES\" >> /aleph/logs/node_status",

                # Write the config.toml file line by line
                "echo '[network]' > /aleph/aleph-node-config.toml",
                "echo 'listen_address = \"0.0.0.0:30333\"' >> /aleph/aleph-node-config.toml",
                f"echo 'ip_manager_address = \"{ip_manager_instance.instance_private_ip}\"' >> /aleph/aleph-node-config.toml",
                "echo \"ip_address = \\\"$PRIVATE_IP\\\"\" >> /aleph/aleph-node-config.toml", 
                "echo \"nodes = [$NODES]\" >> /aleph/aleph-node-config.toml",
                f"echo 'total_nodes = {INSTANCES_NUMBER}' >> /aleph/aleph-node-config.toml",
                "echo '' >> /aleph/aleph-node-config.toml",
                "echo '[consensus]' >> /aleph/aleph-node-config.toml",
                f"echo 'number_of_transactions = {BATCH_SIZE}' >> /aleph/aleph-node-config.toml",
                f"echo 'transaction_size = {TRANSACTION_SIZE} # bytes' >> /aleph/aleph-node-config.toml",
                f"echo 'data_shards = {SHARD_SIZE} # Number of data shards for erasure coding' >> /aleph/aleph-node-config.toml",
                f"echo 'total_rounds = {TOTAL_ROUNDS} # Number of rounds' >> /aleph/aleph-node-config.toml",
                "echo '' >> /aleph/aleph-node-config.toml",
                "echo '[logging]' >> /aleph/aleph-node-config.toml",
                "echo 'level = \"info\"' >> /aleph/aleph-node-config.toml",
                "echo 'transaction_metrics_log = \"/aleph/logs/transaction_metrics\"' >> /aleph/aleph-node-config.toml",
                "echo '' >> /aleph/aleph-node-config.toml",
                "echo '[node]' >> /aleph/aleph-node-config.toml",
                f"echo 'id = {i + 1}' >> /aleph/aleph-node-config.toml",
                "cat /aleph/aleph-node-config.toml >> /aleph/logs/node_status"
            )


            # Part 3: Start aleph_rbc
            ec2_instance.user_data.add_commands(
                "echo 'Starting aleph_rbc execution' >> /aleph/logs/node_status",
                # Check if the aleph_rbc binary is reachable and log the result
                "if [ -f /aleph/aleph_rbc ]; then",
                "  echo 'aleph_rbc binary is found at /aleph/aleph_rbc' >> /aleph/logs/node_status;",
                "else",
                "  echo 'ERROR: aleph_rbc binary not found at /aleph/aleph_rbc' >> /aleph/logs/node_status;",
                "fi",

                # Check if the configuration file is reachable and log the result
                "if [ -f /aleph/aleph-node-config.toml ]; then",
                "  echo 'Configuration file found at /aleph/aleph-node-config.toml' >> /aleph/logs/node_status;",
                "else",
                "  echo 'ERROR: Configuration file not found at /aleph/aleph-node-config.toml' >> /aleph/logs/node_status;",
                "fi",


                # Start the Aleph APIs
                "echo 'Attempting to execute aleph_rbc with configuration' >> /aleph/logs/node_status;",
                # Launch aleph_rbc with FD limits
                "ulimit -n 1048576 && /aleph/aleph_rbc --config /aleph/aleph-node-config.toml >> /aleph/logs/node_status 2>&1 &"
                # "/aleph/aleph_rbc --config /aleph/aleph-node-config.toml >> /aleph/logs/node_status 2>&1 &",

                # Wait for the aleph_rbc server to be ready (simple retry logic)
                "echo 'Waiting for aleph_rbc to be ready on port 30333' >> /aleph/logs/node_status;",
                    # Wait for the aleph_rbc server to be ready (simple retry logic)
                "for i in {1..30}; do",
                "    if netstat -tuln | grep -q ':30333'; then",
                "        echo 'aleph_rbc APIs are ready.' >> /aleph/logs/node_status;",
                "        break;",
                "    fi",
                "    echo 'aleph_rbc not ready, retrying...' >> /aleph/logs/node_status;",
                "    sleep 1;",
                "done",
                
                "if ! netstat -tuln | grep -q ':30333'; then",
                "    echo 'ERROR: aleph_rbc failed to start after 30 retries. Exiting.' >> /aleph/logs/node_status;",
                "    exit 1;",
                "fi",

                # Wait for the aleph_rbc server to confirm readiness via its API"
                "echo 'Waiting for aleph_rbc API readiness...' >> /aleph/logs/node_status;",
                "for i in {1..30}; do",
                "    if curl -s http://127.0.0.1:30333/health | grep -q 'healthy'; then",
                "        echo 'aleph_rbc API is ready.' >> /aleph/logs/node_status;",
                "        break;",
                "    fi",
                "    echo 'aleph_rbc API not ready, retrying...' >> /aleph/logs/node_status;",
                "    sleep 1;",
                "done",
                "",
                "if ! curl -s http://127.0.0.1:30333/health | grep -q 'healthy'; then",
                "    echo 'ERROR: aleph_rbc API failed to start after 30 retries. Exiting.' >> /aleph/logs/node_status;",
                "    exit 1;",
                "fi",

                f"echo 'Done with aleph_rbc loop for node {i + 1} ' >> /aleph/logs/node_status;",
                
            )


            # Output the instance ID for debugging
            CfnOutput(self, f"InstanceIdOutput{i+1}",
                    value=ec2_instance.instance_id,
                    description=f"Instance ID for MyInstance{i+1}")


# App setup
app = App()
TestAleph(app, "TestAleph")  # Instantiate the TestAleph Stack within the app

app.synth()
