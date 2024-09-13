# transaction_proposal_setup.py
# Setup of nodes for transaction proposals and logging configuration.
# What’s Missing/Needed from You:

#     GitHub repository URL: Confirm the repository URL for Aleph protocol.
#     EC2 Key Pair Name: Provide the name of the key pair to use for SSH access.
#     Aleph Configuration Details (ALEPH_CONFIG): Provide or confirm the configuration file details required for Aleph protocol.
#     Node Identifiers and Specific Setup: Specify how nodes should be identified and any other specifics required for setting up transaction proposals.
#     Custom Transaction Sizes and Counts: Confirm the default values or provide new ones if different from the example.
from aws_cdk import (
    aws_ec2 as ec2,
    core
)

class TransactionProposalSetup(core.Stack):
    def __init__(self, scope: core.Construct, id: str, instances, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Setup logging and transaction proposal for each instance
        for i, instance in enumerate(instances):
            instance_id = instance.instance_id

            # Assuming the setup script is part of the user data
            setup_commands = [
                "sudo mkdir -p /var/log/aleph",  # Directory for logs
                "sudo touch /var/log/aleph/transactions.log",  # Log file for transactions
                # Setup transaction proposals - ask user for specifics on the commands and configuration
                "echo 'Configuring transaction proposals...' > /var/log/aleph/setup.log",
                "export NODE_ID=node_{i}",  # Ask user for node identification specifics
                "export TRANSACTION_SIZE=250",  # Example transaction size; ask user if different
                "export TRANSACTION_COUNT=256",  # Example transaction count; ask user if different
                "./target/release/aleph-node --node $NODE_ID --transactions $TRANSACTION_SIZE --count $TRANSACTION_COUNT"  # Run command, ask for details
            ]

            # Add user data commands
            for cmd in setup_commands:
                instance.user_data.add_commands(cmd)
            
            # Expose output for the transaction log location
            core.CfnOutput(self, f"TransactionLogOutput{i+1}",
                           value=f"/var/log/aleph/transactions.log",
                           description=f"Transaction log file for AlephInstance{i+1}")
