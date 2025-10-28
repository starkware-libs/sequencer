# CDK8S Deployment
cdk8s is an open-source software development framework for defining Kubernetes applications,
and reusable abstractions using familiar programming languages and rich object-oriented APIs.  
cdk8s apps synthesize into standard Kubernetes manifests that can be applied to any Kubernetes cluster.

- Official documentation https://cdk8s.io/docs/latest/

## Requirements
Please note: all the requirements instructions are optional.  
You can use any method you like to install the required tools.

### List of required tools
1. python3.10
2. python3.10-pipenv
3. nodejs + npm
4. cdk8s-cli

### Setup python3.10
**For ubuntu 22.04 users**
```bash
    sudo apt update
    sudo apt install python3.10-full 
```
**For ubuntu 24.04 users**
```bash
    sudo add-apt-repository ppa:deadsnakes/ppa
    sudo apt update
    sudo apt install python3.10-full 
```

### Setup nodejs
```bash
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
    # For bash users
    source ~/.bashrc

    # For zsh users
    source ~/.zshrc
    nvm install 22.14.0
    nvm use 22.14.0
```

### Install cdk8s
```bash
    npm install -g cdk8s-cli@2.198.267
```

### Install pipenv
```bash
    pip install pipenv
```

## How to use cdk8s
### initialize cdk8s ( required to execute once )
```bash
    cd deployments/sequencer
    pipenv install
    cdk8s import
```

### Use cdk8s
#### Examples:
```bash
    cd deployments/sequencer
    cdk8s synth --app "pipenv run python main.py --namespace <k8s namespace> --deployment-config-file <path to deployment config> --deployment-image-tag <apollo node image tag>"
```

### Deploy sequencer
cdk8s generated the k8s manifests to `./dist` folder.  
You can now use `kubectl apply` or any other preferred method to deploy.  
