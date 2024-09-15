from aws_cdk import (
    aws_ec2 as ec2,
    core
)

class TransactionProposalSetup(core.Stack):
    def __init__(self, scope: core.Construct, id: str, instances, **kwargs) -> None:
        super().__init__(scope, id, **kwargs)

        # Unified configuration for transaction size and batch size
        transaction_size = 250  # Default transaction size, can be updated
        transaction_count = 256  # Default transaction count, same as batch size

        # Setup logging and transaction proposal for each instance
        for i, instance in enumerate(instances):
            # Assuming the setup script is part of the user data
            setup_commands = [
                "sudo mkdir -p /var/log/aleph",  # Directory for logs
                "sudo touch /var/log/aleph/transactions.log",  # Log file for transactions
                # Setup transaction proposals
                "echo 'Configuring transaction proposals...' > /var/log/aleph/setup.log",
                f"export NODE_ID=node_{i+1}",  # Node identification
                f"export TRANSACTION_SIZE={transaction_size}",  # Unified transaction size
                f"export TRANSACTION_COUNT={transaction_count}",  # Unified transaction count (batch size)
                "./target/release/alephRBC --node $NODE_ID --transactions $TRANSACTION_SIZE --count $TRANSACTION_COUNT"  # Run transaction proposal with unified config
            ]

            # Add user data commands
            for cmd in setup_commands:
                instance.user_data.add_commands(cmd)
            
            # Expose output for the transaction log location
            core.CfnOutput(self, f"TransactionLogOutput{i+1}",
                           value=f"/var/log/aleph/transactions.log",
                           description=f"Transaction log file for AlephInstance{i+1}")
