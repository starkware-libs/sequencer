module example.com/{{ $base }}

go 1.18

require (
	github.com/aws/constructs-go/constructs/v10 v{{ constructs_version }}
	github.com/aws/jsii-runtime-go v{{ jsii_version }}
	github.com/cdk8s-team/cdk8s-core-go/cdk8s/v2 v{{ cdk8s_core_version }}
	github.com/cdk8s-team/cdk8s-plus-go/cdk8splus25/v2 v{{ cdk8s_plus_version }}
)
