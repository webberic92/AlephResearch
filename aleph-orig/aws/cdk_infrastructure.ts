// Import necessary modules from AWS CDK libraries
import * as cdk from 'aws-cdk-lib'; // Import the AWS CDK library
import { Construct } from 'constructs'; // Import Construct, which is the base class for all constructs
import * as ec2 from 'aws-cdk-lib/aws-ec2'; // Import the EC2 module for creating a VPC and related networking resources
import * as rds from 'aws-cdk-lib/aws-rds'; // Import the RDS module for creating a PostgreSQL database instance
import * as ssm from 'aws-cdk-lib/aws-ssm'; // Import the SSM module for storing parameters in AWS Systems Manager Parameter Store

// Define a new CDK Stack class called AlephProtocolStack
export class AlephProtocolStack extends cdk.Stack {
    // Constructor for the AlephProtocolStack class
    constructor(scope: Construct, id: string, props?: cdk.StackProps) {
        // Call the parent class constructor
        super(scope, id, props);

        // Create a new VPC (Virtual Private Cloud) for the stack
        const vpc = new ec2.Vpc(this, 'AlephVpc', {
            maxAzs: 2, // Specify the maximum number of Availability Zones to use (2 in this case)
            natGateways: 1, // Specify the number of NAT Gateways to create (1 in this case)
        });

        // Create a new PostgreSQL database instance in the VPC
        const dbInstance = new rds.DatabaseInstance(this, 'AlephPostgres', {
            engine: rds.DatabaseInstanceEngine.postgres({
                version: rds.PostgresEngineVersion.VER_12_20, // Specify the version of PostgreSQL to use
            }),
            instanceType: ec2.InstanceType.of(ec2.InstanceClass.BURSTABLE2, ec2.InstanceSize.MICRO), // Specify the instance type (t2.micro in this case)
            vpc, // Associate the database instance with the VPC created earlier
            multiAz: false, // Disable Multi-AZ deployment (set to false)
            allocatedStorage: 20, // Specify the amount of storage to allocate for the database (20 GB)
            storageType: rds.StorageType.GP2, // Specify the storage type to use (General Purpose SSD)
            publiclyAccessible: false, // Set the database instance to be private (not publicly accessible)
            credentials: rds.Credentials.fromGeneratedSecret('postgres'), // Automatically generate and use credentials for the 'postgres' user
        });

        // Create a new Systems Manager (SSM) Parameter to store the database endpoint
        new ssm.StringParameter(this, 'DbEndpoint', {
            parameterName: '/aleph-protocol/db-endpoint', // Define the name of the parameter
            stringValue: dbInstance.instanceEndpoint.socketAddress, // Store the database instance endpoint address as the parameter value
        });
    }
}

// Create a new CDK application
const app = new cdk.App();

// Instantiate a new AlephProtocolStack within the app
new AlephProtocolStack(app, 'AlephProtocolStack');
