Local monitoring stack
first enable pipenv:

```bash
pipenv --python 3.12
pipenv shell
```

to deploy run:
```bash
./deploy_local_stack.sh up -d
```
This will deploy node, Promethous and Grafana containers and upload the src/dummy_json.json dashboard to the grafana


to destroy:
```bash
./deploy_local_stack.sh down
```

to remove pipenv:
```bash
pipenv --rm
```