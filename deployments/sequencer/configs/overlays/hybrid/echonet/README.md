# Echonet, hybrid layout

Runs the echonet sequencer as the six hybrid services instead of one consolidated node.

Needs an image that includes #15183 (set `image.tag` in `common.yaml`) and echonet with #15184.

## One-time switch from consolidated

1. Put the node secret file at `node_secrets.json` next to this README (gitignored): the private params
   listed in `crates/apollo_node/resources/config_secrets_schema.json`, as flat dotted keys. It is the
   same file the consolidated echonet node uses.
2. Synthesize: `cdk8s synth --app "pipenv run python main.py --namespace <ns> -l hybrid -o hybrid.echonet"`.
3. Stop the consolidated node, whose disk core takes over:
   `kubectl -n <ns> scale statefulset/sequencer-node-statefulset --replicas=0`.
4. Seed the committer disk. Core keeps the rest of the storage on `sequencer-node-data`, but the
   committer runs on the new `sequencer-committer-data` and must start from a copy of `/data/committer`.
   The copy pod is the disk's first consumer, so it is created in the source disk's zone:

   ```bash
   kubectl -n <ns> apply -f dist/sequencer-committer/PersistentVolumeClaim.sequencer-committer-data.k8s.yaml
   kubectl -n <ns> apply -f - <<'EOF'
   apiVersion: v1
   kind: Pod
   metadata:
     name: seed-committer-data
   spec:
     restartPolicy: Never
     nodeSelector: {role: sequencer, topology.kubernetes.io/zone: us-central1-f}
     tolerations: [{key: role, operator: Equal, value: sequencer, effect: NoSchedule}]
     containers:
       - name: copy
         image: alpine:3.20
         command: ["sh", "-c", "cp -a /src/committer /dst/ && du -sh /dst/committer"]
         volumeMounts:
           - {name: src, mountPath: /src, readOnly: true}
           - {name: dst, mountPath: /dst}
     volumes:
       - {name: src, persistentVolumeClaim: {claimName: sequencer-node-data, readOnly: true}}
       - {name: dst, persistentVolumeClaim: {claimName: sequencer-committer-data}}
   EOF
   kubectl -n <ns> wait pod/seed-committer-data --for=jsonpath='{.status.phase}'=Succeeded --timeout=2h
   kubectl -n <ns> logs seed-committer-data && kubectl -n <ns> delete pod seed-committer-data
   ```

5. Set `"sequencer_layout": "hybrid"` in `echonet/echonet_keys.json`.

## Deploy

```bash
kubectl -n <ns> apply -R -f ./dist
python echonet/deploy_echonet.py -n <ns>
```
