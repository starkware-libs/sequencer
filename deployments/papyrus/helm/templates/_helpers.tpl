{{/*
Expand the name of the chart.
*/}}
{{- define "papyrus.name" -}}
{{- default .Release.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "papyrus.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "papyrus.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "papyrus.labels" -}}
helm.sh/chart: {{ include "papyrus.chart" . }}
app: {{ include "papyrus.name" . }}
{{ include "papyrus.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "papyrus.selectorLabels" -}}
app.kubernetes.io/name: {{ include "papyrus.name" . }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "papyrus.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "papyrus.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Build the bootstrap p2p peer multiaddress string
*/}}
{{- define "p2p.bootstrapPeerMultiaddr" -}}
{{- if and .Values.p2p.enabled (not .Values.p2p.bootstrap) -}}
  {{- $ip :=  .Values.p2p.nodeConfig.bootstrapServer.multiaddrIp -}}
  {{- $port := int .Values.p2p.nodeConfig.bootstrapServer.multiaddrPort -}}
  {{- $uid :=  .Values.p2p.nodeConfig.bootstrapServer.multiaddrUid -}}
  {{- printf "/ip4/%s/tcp/%d/p2p/%s" $ip $port $uid -}}
{{- else -}}
  {{- "" -}}
{{- end -}}
{{- end -}}

{{/*
Build the node self peer multiaddress string
*/}}
{{- define "p2p.nodePeerMultiaddr" -}}
{{- if .Values.p2p.enabled -}}
  {{- if .Values.p2p.nodeConfig.overrideIP -}}
    {{- $ip := .Values.p2p.nodeConfig.overrideIP -}}
    {{- $port := int .Values.p2p.config.networkTcpPort -}}
    {{- printf "/ip4/%s/tcp/%d" $ip $port -}}
  {{- else -}}
    {{- if and .Values.p2p.service.enabled (not .Values.p2p.bootstrap) (or .Values.p2p.service.clusterIP .Values.p2p.service.loadBalancerIP) -}}
      {{- $ip := "" -}}
      {{- if and (eq .Values.p2p.service.type "ClusterIP") .Values.p2p.service.clusterIP -}}
        {{- $ip = .Values.p2p.service.clusterIP -}}
      {{- end -}}
      {{- if and (eq .Values.p2p.service.type "LoadBalancer") .Values.p2p.service.loadBalancerIP -}}
        {{- $ip = .Values.p2p.service.loadBalancerIP -}}
      {{- end -}}
      {{- $port := int .Values.p2p.service.port -}}
      {{- printf "/ip4/%s/tcp/%d" $ip $port -}}
    {{- end -}}
  {{- end -}}
{{- else -}}
  {{- "" -}}
{{- end -}}
{{- end -}}