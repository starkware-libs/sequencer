from pkgutil import extend_path
__path__ = extend_path(__path__, __name__)

import abc
import builtins
import datetime
import enum
import typing

import jsii
import publication
import typing_extensions

import typeguard
from importlib.metadata import version as _metadata_package_version
TYPEGUARD_MAJOR_VERSION = int(_metadata_package_version('typeguard').split('.')[0])

def check_type(argname: str, value: object, expected_type: typing.Any) -> typing.Any:
    if TYPEGUARD_MAJOR_VERSION <= 2:
        return typeguard.check_type(argname=argname, value=value, expected_type=expected_type) # type:ignore
    else:
        if isinstance(value, jsii._reference_map.InterfaceDynamicProxy): # pyright: ignore [reportAttributeAccessIssue]
           pass
        else:
            if TYPEGUARD_MAJOR_VERSION == 3:
                typeguard.config.collection_check_strategy = typeguard.CollectionCheckStrategy.ALL_ITEMS # type:ignore
                typeguard.check_type(value=value, expected_type=expected_type) # type:ignore
            else:
                typeguard.check_type(value=value, expected_type=expected_type, collection_check_strategy=typeguard.CollectionCheckStrategy.ALL_ITEMS) # type:ignore

from ._jsii import *

import cdk8s as _cdk8s_d3d9af27
import constructs as _constructs_77d1e7e8


class BackendConfig(
    _cdk8s_d3d9af27.ApiObject,
    metaclass=jsii.JSIIMeta,
    jsii_type="comgooglecloud.BackendConfig",
):
    '''
    :schema: BackendConfig
    '''

    def __init__(
        self,
        scope: _constructs_77d1e7e8.Construct,
        id: builtins.str,
        *,
        metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
        spec: typing.Optional[typing.Union["BackendConfigSpec", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''Defines a "BackendConfig" API object.

        :param scope: the scope in which to define this object.
        :param id: a scope-local name for the object.
        :param metadata: 
        :param spec: BackendConfigSpec is the spec for a BackendConfig resource.
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__478e053b34646f2f22c316b46d46fa578352a17dc0965e0733c3edbb3af21e40)
            check_type(argname="argument scope", value=scope, expected_type=type_hints["scope"])
            check_type(argname="argument id", value=id, expected_type=type_hints["id"])
        props = BackendConfigProps(metadata=metadata, spec=spec)

        jsii.create(self.__class__, self, [scope, id, props])

    @jsii.member(jsii_name="manifest")
    @builtins.classmethod
    def manifest(
        cls,
        *,
        metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
        spec: typing.Optional[typing.Union["BackendConfigSpec", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> typing.Any:
        '''Renders a Kubernetes manifest for "BackendConfig".

        This can be used to inline resource manifests inside other objects (e.g. as templates).

        :param metadata: 
        :param spec: BackendConfigSpec is the spec for a BackendConfig resource.
        '''
        props = BackendConfigProps(metadata=metadata, spec=spec)

        return typing.cast(typing.Any, jsii.sinvoke(cls, "manifest", [props]))

    @jsii.member(jsii_name="toJson")
    def to_json(self) -> typing.Any:
        '''Renders the object to Kubernetes JSON.'''
        return typing.cast(typing.Any, jsii.invoke(self, "toJson", []))

    @jsii.python.classproperty
    @jsii.member(jsii_name="GVK")
    def GVK(cls) -> _cdk8s_d3d9af27.GroupVersionKind:
        '''Returns the apiVersion and kind for "BackendConfig".'''
        return typing.cast(_cdk8s_d3d9af27.GroupVersionKind, jsii.sget(cls, "GVK"))


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigProps",
    jsii_struct_bases=[],
    name_mapping={"metadata": "metadata", "spec": "spec"},
)
class BackendConfigProps:
    def __init__(
        self,
        *,
        metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
        spec: typing.Optional[typing.Union["BackendConfigSpec", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''
        :param metadata: 
        :param spec: BackendConfigSpec is the spec for a BackendConfig resource.

        :schema: BackendConfig
        '''
        if isinstance(metadata, dict):
            metadata = _cdk8s_d3d9af27.ApiObjectMetadata(**metadata)
        if isinstance(spec, dict):
            spec = BackendConfigSpec(**spec)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__e8e7a740219c8083806cfb80142ff0861537e30e169b215266207dac2fc18296)
            check_type(argname="argument metadata", value=metadata, expected_type=type_hints["metadata"])
            check_type(argname="argument spec", value=spec, expected_type=type_hints["spec"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if metadata is not None:
            self._values["metadata"] = metadata
        if spec is not None:
            self._values["spec"] = spec

    @builtins.property
    def metadata(self) -> typing.Optional[_cdk8s_d3d9af27.ApiObjectMetadata]:
        '''
        :schema: BackendConfig#metadata
        '''
        result = self._values.get("metadata")
        return typing.cast(typing.Optional[_cdk8s_d3d9af27.ApiObjectMetadata], result)

    @builtins.property
    def spec(self) -> typing.Optional["BackendConfigSpec"]:
        '''BackendConfigSpec is the spec for a BackendConfig resource.

        :schema: BackendConfig#spec
        '''
        result = self._values.get("spec")
        return typing.cast(typing.Optional["BackendConfigSpec"], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigProps(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpec",
    jsii_struct_bases=[],
    name_mapping={
        "cdn": "cdn",
        "connection_draining": "connectionDraining",
        "custom_request_headers": "customRequestHeaders",
        "custom_response_headers": "customResponseHeaders",
        "health_check": "healthCheck",
        "iap": "iap",
        "logging": "logging",
        "security_policy": "securityPolicy",
        "session_affinity": "sessionAffinity",
        "timeout_sec": "timeoutSec",
    },
)
class BackendConfigSpec:
    def __init__(
        self,
        *,
        cdn: typing.Optional[typing.Union["BackendConfigSpecCdn", typing.Dict[builtins.str, typing.Any]]] = None,
        connection_draining: typing.Optional[typing.Union["BackendConfigSpecConnectionDraining", typing.Dict[builtins.str, typing.Any]]] = None,
        custom_request_headers: typing.Optional[typing.Union["BackendConfigSpecCustomRequestHeaders", typing.Dict[builtins.str, typing.Any]]] = None,
        custom_response_headers: typing.Optional[typing.Union["BackendConfigSpecCustomResponseHeaders", typing.Dict[builtins.str, typing.Any]]] = None,
        health_check: typing.Optional[typing.Union["BackendConfigSpecHealthCheck", typing.Dict[builtins.str, typing.Any]]] = None,
        iap: typing.Optional[typing.Union["BackendConfigSpecIap", typing.Dict[builtins.str, typing.Any]]] = None,
        logging: typing.Optional[typing.Union["BackendConfigSpecLogging", typing.Dict[builtins.str, typing.Any]]] = None,
        security_policy: typing.Optional[typing.Union["BackendConfigSpecSecurityPolicy", typing.Dict[builtins.str, typing.Any]]] = None,
        session_affinity: typing.Optional[typing.Union["BackendConfigSpecSessionAffinity", typing.Dict[builtins.str, typing.Any]]] = None,
        timeout_sec: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''BackendConfigSpec is the spec for a BackendConfig resource.

        :param cdn: CDNConfig contains configuration for CDN-enabled backends.
        :param connection_draining: ConnectionDrainingConfig contains configuration for connection draining. For now the draining timeout. May manage more settings in the future.
        :param custom_request_headers: CustomRequestHeadersConfig contains configuration for custom request headers.
        :param custom_response_headers: CustomResponseHeadersConfig contains configuration for custom response headers.
        :param health_check: HealthCheckConfig contains configuration for the health check.
        :param iap: IAPConfig contains configuration for IAP-enabled backends.
        :param logging: LogConfig contains configuration for logging.
        :param security_policy: SecurityPolicyConfig contains configuration for CloudArmor-enabled backends. If not specified, the controller will not reconcile the security policy configuration. In other words, users can make changes in GCE without the controller overwriting them.
        :param session_affinity: SessionAffinityConfig contains configuration for stickiness parameters.
        :param timeout_sec: 

        :schema: BackendConfigSpec
        '''
        if isinstance(cdn, dict):
            cdn = BackendConfigSpecCdn(**cdn)
        if isinstance(connection_draining, dict):
            connection_draining = BackendConfigSpecConnectionDraining(**connection_draining)
        if isinstance(custom_request_headers, dict):
            custom_request_headers = BackendConfigSpecCustomRequestHeaders(**custom_request_headers)
        if isinstance(custom_response_headers, dict):
            custom_response_headers = BackendConfigSpecCustomResponseHeaders(**custom_response_headers)
        if isinstance(health_check, dict):
            health_check = BackendConfigSpecHealthCheck(**health_check)
        if isinstance(iap, dict):
            iap = BackendConfigSpecIap(**iap)
        if isinstance(logging, dict):
            logging = BackendConfigSpecLogging(**logging)
        if isinstance(security_policy, dict):
            security_policy = BackendConfigSpecSecurityPolicy(**security_policy)
        if isinstance(session_affinity, dict):
            session_affinity = BackendConfigSpecSessionAffinity(**session_affinity)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__ed3aaa52e5e9ad8c8ce8edc12e3f41b1303bc88c9e4d6d58ade780d4014f2019)
            check_type(argname="argument cdn", value=cdn, expected_type=type_hints["cdn"])
            check_type(argname="argument connection_draining", value=connection_draining, expected_type=type_hints["connection_draining"])
            check_type(argname="argument custom_request_headers", value=custom_request_headers, expected_type=type_hints["custom_request_headers"])
            check_type(argname="argument custom_response_headers", value=custom_response_headers, expected_type=type_hints["custom_response_headers"])
            check_type(argname="argument health_check", value=health_check, expected_type=type_hints["health_check"])
            check_type(argname="argument iap", value=iap, expected_type=type_hints["iap"])
            check_type(argname="argument logging", value=logging, expected_type=type_hints["logging"])
            check_type(argname="argument security_policy", value=security_policy, expected_type=type_hints["security_policy"])
            check_type(argname="argument session_affinity", value=session_affinity, expected_type=type_hints["session_affinity"])
            check_type(argname="argument timeout_sec", value=timeout_sec, expected_type=type_hints["timeout_sec"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if cdn is not None:
            self._values["cdn"] = cdn
        if connection_draining is not None:
            self._values["connection_draining"] = connection_draining
        if custom_request_headers is not None:
            self._values["custom_request_headers"] = custom_request_headers
        if custom_response_headers is not None:
            self._values["custom_response_headers"] = custom_response_headers
        if health_check is not None:
            self._values["health_check"] = health_check
        if iap is not None:
            self._values["iap"] = iap
        if logging is not None:
            self._values["logging"] = logging
        if security_policy is not None:
            self._values["security_policy"] = security_policy
        if session_affinity is not None:
            self._values["session_affinity"] = session_affinity
        if timeout_sec is not None:
            self._values["timeout_sec"] = timeout_sec

    @builtins.property
    def cdn(self) -> typing.Optional["BackendConfigSpecCdn"]:
        '''CDNConfig contains configuration for CDN-enabled backends.

        :schema: BackendConfigSpec#cdn
        '''
        result = self._values.get("cdn")
        return typing.cast(typing.Optional["BackendConfigSpecCdn"], result)

    @builtins.property
    def connection_draining(
        self,
    ) -> typing.Optional["BackendConfigSpecConnectionDraining"]:
        '''ConnectionDrainingConfig contains configuration for connection draining.

        For now the draining timeout. May manage more settings in the future.

        :schema: BackendConfigSpec#connectionDraining
        '''
        result = self._values.get("connection_draining")
        return typing.cast(typing.Optional["BackendConfigSpecConnectionDraining"], result)

    @builtins.property
    def custom_request_headers(
        self,
    ) -> typing.Optional["BackendConfigSpecCustomRequestHeaders"]:
        '''CustomRequestHeadersConfig contains configuration for custom request headers.

        :schema: BackendConfigSpec#customRequestHeaders
        '''
        result = self._values.get("custom_request_headers")
        return typing.cast(typing.Optional["BackendConfigSpecCustomRequestHeaders"], result)

    @builtins.property
    def custom_response_headers(
        self,
    ) -> typing.Optional["BackendConfigSpecCustomResponseHeaders"]:
        '''CustomResponseHeadersConfig contains configuration for custom response headers.

        :schema: BackendConfigSpec#customResponseHeaders
        '''
        result = self._values.get("custom_response_headers")
        return typing.cast(typing.Optional["BackendConfigSpecCustomResponseHeaders"], result)

    @builtins.property
    def health_check(self) -> typing.Optional["BackendConfigSpecHealthCheck"]:
        '''HealthCheckConfig contains configuration for the health check.

        :schema: BackendConfigSpec#healthCheck
        '''
        result = self._values.get("health_check")
        return typing.cast(typing.Optional["BackendConfigSpecHealthCheck"], result)

    @builtins.property
    def iap(self) -> typing.Optional["BackendConfigSpecIap"]:
        '''IAPConfig contains configuration for IAP-enabled backends.

        :schema: BackendConfigSpec#iap
        '''
        result = self._values.get("iap")
        return typing.cast(typing.Optional["BackendConfigSpecIap"], result)

    @builtins.property
    def logging(self) -> typing.Optional["BackendConfigSpecLogging"]:
        '''LogConfig contains configuration for logging.

        :schema: BackendConfigSpec#logging
        '''
        result = self._values.get("logging")
        return typing.cast(typing.Optional["BackendConfigSpecLogging"], result)

    @builtins.property
    def security_policy(self) -> typing.Optional["BackendConfigSpecSecurityPolicy"]:
        '''SecurityPolicyConfig contains configuration for CloudArmor-enabled backends.

        If not specified, the controller will not reconcile the security policy configuration. In other words, users can make changes in GCE without the controller overwriting them.

        :schema: BackendConfigSpec#securityPolicy
        '''
        result = self._values.get("security_policy")
        return typing.cast(typing.Optional["BackendConfigSpecSecurityPolicy"], result)

    @builtins.property
    def session_affinity(self) -> typing.Optional["BackendConfigSpecSessionAffinity"]:
        '''SessionAffinityConfig contains configuration for stickiness parameters.

        :schema: BackendConfigSpec#sessionAffinity
        '''
        result = self._values.get("session_affinity")
        return typing.cast(typing.Optional["BackendConfigSpecSessionAffinity"], result)

    @builtins.property
    def timeout_sec(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpec#timeoutSec
        '''
        result = self._values.get("timeout_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpec(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCdn",
    jsii_struct_bases=[],
    name_mapping={
        "enabled": "enabled",
        "bypass_cache_on_request_headers": "bypassCacheOnRequestHeaders",
        "cache_mode": "cacheMode",
        "cache_policy": "cachePolicy",
        "client_ttl": "clientTtl",
        "default_ttl": "defaultTtl",
        "max_ttl": "maxTtl",
        "negative_caching": "negativeCaching",
        "negative_caching_policy": "negativeCachingPolicy",
        "request_coalescing": "requestCoalescing",
        "serve_while_stale": "serveWhileStale",
        "signed_url_cache_max_age_sec": "signedUrlCacheMaxAgeSec",
        "signed_url_keys": "signedUrlKeys",
    },
)
class BackendConfigSpecCdn:
    def __init__(
        self,
        *,
        enabled: builtins.bool,
        bypass_cache_on_request_headers: typing.Optional[typing.Sequence[typing.Union["BackendConfigSpecCdnBypassCacheOnRequestHeaders", typing.Dict[builtins.str, typing.Any]]]] = None,
        cache_mode: typing.Optional[builtins.str] = None,
        cache_policy: typing.Optional[typing.Union["BackendConfigSpecCdnCachePolicy", typing.Dict[builtins.str, typing.Any]]] = None,
        client_ttl: typing.Optional[jsii.Number] = None,
        default_ttl: typing.Optional[jsii.Number] = None,
        max_ttl: typing.Optional[jsii.Number] = None,
        negative_caching: typing.Optional[builtins.bool] = None,
        negative_caching_policy: typing.Optional[typing.Sequence[typing.Union["BackendConfigSpecCdnNegativeCachingPolicy", typing.Dict[builtins.str, typing.Any]]]] = None,
        request_coalescing: typing.Optional[builtins.bool] = None,
        serve_while_stale: typing.Optional[jsii.Number] = None,
        signed_url_cache_max_age_sec: typing.Optional[jsii.Number] = None,
        signed_url_keys: typing.Optional[typing.Sequence[typing.Union["BackendConfigSpecCdnSignedUrlKeys", typing.Dict[builtins.str, typing.Any]]]] = None,
    ) -> None:
        '''CDNConfig contains configuration for CDN-enabled backends.

        :param enabled: 
        :param bypass_cache_on_request_headers: 
        :param cache_mode: 
        :param cache_policy: CacheKeyPolicy contains configuration for how requests to a CDN-enabled backend are cached.
        :param client_ttl: 
        :param default_ttl: 
        :param max_ttl: 
        :param negative_caching: 
        :param negative_caching_policy: 
        :param request_coalescing: 
        :param serve_while_stale: 
        :param signed_url_cache_max_age_sec: 
        :param signed_url_keys: 

        :schema: BackendConfigSpecCdn
        '''
        if isinstance(cache_policy, dict):
            cache_policy = BackendConfigSpecCdnCachePolicy(**cache_policy)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__622a4ce6676e87a1185b566cba87d43a91644ca1bc40dd893f4fac498eb82531)
            check_type(argname="argument enabled", value=enabled, expected_type=type_hints["enabled"])
            check_type(argname="argument bypass_cache_on_request_headers", value=bypass_cache_on_request_headers, expected_type=type_hints["bypass_cache_on_request_headers"])
            check_type(argname="argument cache_mode", value=cache_mode, expected_type=type_hints["cache_mode"])
            check_type(argname="argument cache_policy", value=cache_policy, expected_type=type_hints["cache_policy"])
            check_type(argname="argument client_ttl", value=client_ttl, expected_type=type_hints["client_ttl"])
            check_type(argname="argument default_ttl", value=default_ttl, expected_type=type_hints["default_ttl"])
            check_type(argname="argument max_ttl", value=max_ttl, expected_type=type_hints["max_ttl"])
            check_type(argname="argument negative_caching", value=negative_caching, expected_type=type_hints["negative_caching"])
            check_type(argname="argument negative_caching_policy", value=negative_caching_policy, expected_type=type_hints["negative_caching_policy"])
            check_type(argname="argument request_coalescing", value=request_coalescing, expected_type=type_hints["request_coalescing"])
            check_type(argname="argument serve_while_stale", value=serve_while_stale, expected_type=type_hints["serve_while_stale"])
            check_type(argname="argument signed_url_cache_max_age_sec", value=signed_url_cache_max_age_sec, expected_type=type_hints["signed_url_cache_max_age_sec"])
            check_type(argname="argument signed_url_keys", value=signed_url_keys, expected_type=type_hints["signed_url_keys"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "enabled": enabled,
        }
        if bypass_cache_on_request_headers is not None:
            self._values["bypass_cache_on_request_headers"] = bypass_cache_on_request_headers
        if cache_mode is not None:
            self._values["cache_mode"] = cache_mode
        if cache_policy is not None:
            self._values["cache_policy"] = cache_policy
        if client_ttl is not None:
            self._values["client_ttl"] = client_ttl
        if default_ttl is not None:
            self._values["default_ttl"] = default_ttl
        if max_ttl is not None:
            self._values["max_ttl"] = max_ttl
        if negative_caching is not None:
            self._values["negative_caching"] = negative_caching
        if negative_caching_policy is not None:
            self._values["negative_caching_policy"] = negative_caching_policy
        if request_coalescing is not None:
            self._values["request_coalescing"] = request_coalescing
        if serve_while_stale is not None:
            self._values["serve_while_stale"] = serve_while_stale
        if signed_url_cache_max_age_sec is not None:
            self._values["signed_url_cache_max_age_sec"] = signed_url_cache_max_age_sec
        if signed_url_keys is not None:
            self._values["signed_url_keys"] = signed_url_keys

    @builtins.property
    def enabled(self) -> builtins.bool:
        '''
        :schema: BackendConfigSpecCdn#enabled
        '''
        result = self._values.get("enabled")
        assert result is not None, "Required property 'enabled' is missing"
        return typing.cast(builtins.bool, result)

    @builtins.property
    def bypass_cache_on_request_headers(
        self,
    ) -> typing.Optional[typing.List["BackendConfigSpecCdnBypassCacheOnRequestHeaders"]]:
        '''
        :schema: BackendConfigSpecCdn#bypassCacheOnRequestHeaders
        '''
        result = self._values.get("bypass_cache_on_request_headers")
        return typing.cast(typing.Optional[typing.List["BackendConfigSpecCdnBypassCacheOnRequestHeaders"]], result)

    @builtins.property
    def cache_mode(self) -> typing.Optional[builtins.str]:
        '''
        :schema: BackendConfigSpecCdn#cacheMode
        '''
        result = self._values.get("cache_mode")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def cache_policy(self) -> typing.Optional["BackendConfigSpecCdnCachePolicy"]:
        '''CacheKeyPolicy contains configuration for how requests to a CDN-enabled backend are cached.

        :schema: BackendConfigSpecCdn#cachePolicy
        '''
        result = self._values.get("cache_policy")
        return typing.cast(typing.Optional["BackendConfigSpecCdnCachePolicy"], result)

    @builtins.property
    def client_ttl(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpecCdn#clientTtl
        '''
        result = self._values.get("client_ttl")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def default_ttl(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpecCdn#defaultTtl
        '''
        result = self._values.get("default_ttl")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def max_ttl(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpecCdn#maxTtl
        '''
        result = self._values.get("max_ttl")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def negative_caching(self) -> typing.Optional[builtins.bool]:
        '''
        :schema: BackendConfigSpecCdn#negativeCaching
        '''
        result = self._values.get("negative_caching")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def negative_caching_policy(
        self,
    ) -> typing.Optional[typing.List["BackendConfigSpecCdnNegativeCachingPolicy"]]:
        '''
        :schema: BackendConfigSpecCdn#negativeCachingPolicy
        '''
        result = self._values.get("negative_caching_policy")
        return typing.cast(typing.Optional[typing.List["BackendConfigSpecCdnNegativeCachingPolicy"]], result)

    @builtins.property
    def request_coalescing(self) -> typing.Optional[builtins.bool]:
        '''
        :schema: BackendConfigSpecCdn#requestCoalescing
        '''
        result = self._values.get("request_coalescing")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def serve_while_stale(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpecCdn#serveWhileStale
        '''
        result = self._values.get("serve_while_stale")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def signed_url_cache_max_age_sec(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpecCdn#signedUrlCacheMaxAgeSec
        '''
        result = self._values.get("signed_url_cache_max_age_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def signed_url_keys(
        self,
    ) -> typing.Optional[typing.List["BackendConfigSpecCdnSignedUrlKeys"]]:
        '''
        :schema: BackendConfigSpecCdn#signedUrlKeys
        '''
        result = self._values.get("signed_url_keys")
        return typing.cast(typing.Optional[typing.List["BackendConfigSpecCdnSignedUrlKeys"]], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCdn(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCdnBypassCacheOnRequestHeaders",
    jsii_struct_bases=[],
    name_mapping={"header_name": "headerName"},
)
class BackendConfigSpecCdnBypassCacheOnRequestHeaders:
    def __init__(self, *, header_name: typing.Optional[builtins.str] = None) -> None:
        '''BypassCacheOnRequestHeader contains configuration for how requests containing specific request headers bypass the cache, even if the content was previously cached.

        :param header_name: The header field name to match on when bypassing cache. Values are case-insensitive.

        :schema: BackendConfigSpecCdnBypassCacheOnRequestHeaders
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__29143d667e3ffdd72b7660770c144cc283425c836b9727b5dce6d9ad54507331)
            check_type(argname="argument header_name", value=header_name, expected_type=type_hints["header_name"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if header_name is not None:
            self._values["header_name"] = header_name

    @builtins.property
    def header_name(self) -> typing.Optional[builtins.str]:
        '''The header field name to match on when bypassing cache.

        Values are case-insensitive.

        :schema: BackendConfigSpecCdnBypassCacheOnRequestHeaders#headerName
        '''
        result = self._values.get("header_name")
        return typing.cast(typing.Optional[builtins.str], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCdnBypassCacheOnRequestHeaders(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCdnCachePolicy",
    jsii_struct_bases=[],
    name_mapping={
        "include_host": "includeHost",
        "include_protocol": "includeProtocol",
        "include_query_string": "includeQueryString",
        "query_string_blacklist": "queryStringBlacklist",
        "query_string_whitelist": "queryStringWhitelist",
    },
)
class BackendConfigSpecCdnCachePolicy:
    def __init__(
        self,
        *,
        include_host: typing.Optional[builtins.bool] = None,
        include_protocol: typing.Optional[builtins.bool] = None,
        include_query_string: typing.Optional[builtins.bool] = None,
        query_string_blacklist: typing.Optional[typing.Sequence[builtins.str]] = None,
        query_string_whitelist: typing.Optional[typing.Sequence[builtins.str]] = None,
    ) -> None:
        '''CacheKeyPolicy contains configuration for how requests to a CDN-enabled backend are cached.

        :param include_host: If true, requests to different hosts will be cached separately.
        :param include_protocol: If true, http and https requests will be cached separately.
        :param include_query_string: If true, query string parameters are included in the cache key according to QueryStringBlacklist and QueryStringWhitelist. If neither is set, the entire query string is included and if false the entire query string is excluded.
        :param query_string_blacklist: Names of query strint parameters to exclude from cache keys. All other parameters are included. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.
        :param query_string_whitelist: Names of query string parameters to include in cache keys. All other parameters are excluded. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.

        :schema: BackendConfigSpecCdnCachePolicy
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__58baffb66190c78e8d4c5f00de26d9625e7d2a0a6d8e74e5662211067b562a23)
            check_type(argname="argument include_host", value=include_host, expected_type=type_hints["include_host"])
            check_type(argname="argument include_protocol", value=include_protocol, expected_type=type_hints["include_protocol"])
            check_type(argname="argument include_query_string", value=include_query_string, expected_type=type_hints["include_query_string"])
            check_type(argname="argument query_string_blacklist", value=query_string_blacklist, expected_type=type_hints["query_string_blacklist"])
            check_type(argname="argument query_string_whitelist", value=query_string_whitelist, expected_type=type_hints["query_string_whitelist"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if include_host is not None:
            self._values["include_host"] = include_host
        if include_protocol is not None:
            self._values["include_protocol"] = include_protocol
        if include_query_string is not None:
            self._values["include_query_string"] = include_query_string
        if query_string_blacklist is not None:
            self._values["query_string_blacklist"] = query_string_blacklist
        if query_string_whitelist is not None:
            self._values["query_string_whitelist"] = query_string_whitelist

    @builtins.property
    def include_host(self) -> typing.Optional[builtins.bool]:
        '''If true, requests to different hosts will be cached separately.

        :schema: BackendConfigSpecCdnCachePolicy#includeHost
        '''
        result = self._values.get("include_host")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def include_protocol(self) -> typing.Optional[builtins.bool]:
        '''If true, http and https requests will be cached separately.

        :schema: BackendConfigSpecCdnCachePolicy#includeProtocol
        '''
        result = self._values.get("include_protocol")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def include_query_string(self) -> typing.Optional[builtins.bool]:
        '''If true, query string parameters are included in the cache key according to QueryStringBlacklist and QueryStringWhitelist.

        If neither is set, the entire query string is included and if false the entire query string is excluded.

        :schema: BackendConfigSpecCdnCachePolicy#includeQueryString
        '''
        result = self._values.get("include_query_string")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def query_string_blacklist(self) -> typing.Optional[typing.List[builtins.str]]:
        '''Names of query strint parameters to exclude from cache keys.

        All other parameters are included. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.

        :schema: BackendConfigSpecCdnCachePolicy#queryStringBlacklist
        '''
        result = self._values.get("query_string_blacklist")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    @builtins.property
    def query_string_whitelist(self) -> typing.Optional[typing.List[builtins.str]]:
        '''Names of query string parameters to include in cache keys.

        All other parameters are excluded. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.

        :schema: BackendConfigSpecCdnCachePolicy#queryStringWhitelist
        '''
        result = self._values.get("query_string_whitelist")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCdnCachePolicy(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCdnNegativeCachingPolicy",
    jsii_struct_bases=[],
    name_mapping={"code": "code", "ttl": "ttl"},
)
class BackendConfigSpecCdnNegativeCachingPolicy:
    def __init__(
        self,
        *,
        code: typing.Optional[jsii.Number] = None,
        ttl: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''NegativeCachingPolicy contains configuration for how negative caching is applied.

        :param code: The HTTP status code to define a TTL against. Only HTTP status codes 300, 301, 308, 404, 405, 410, 421, 451 and 501 are can be specified as values, and you cannot specify a status code more than once.
        :param ttl: The TTL (in seconds) for which to cache responses with the corresponding status code. The maximum allowed value is 1800s (30 minutes), noting that infrequently accessed objects may be evicted from the cache before the defined TTL.

        :schema: BackendConfigSpecCdnNegativeCachingPolicy
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__7df89c988b317eecced15460557d2535638cb9d58470bd2e37718341ca8102b6)
            check_type(argname="argument code", value=code, expected_type=type_hints["code"])
            check_type(argname="argument ttl", value=ttl, expected_type=type_hints["ttl"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if code is not None:
            self._values["code"] = code
        if ttl is not None:
            self._values["ttl"] = ttl

    @builtins.property
    def code(self) -> typing.Optional[jsii.Number]:
        '''The HTTP status code to define a TTL against.

        Only HTTP status codes 300, 301, 308, 404, 405, 410, 421, 451 and 501 are can be specified as values, and you cannot specify a status code more than once.

        :schema: BackendConfigSpecCdnNegativeCachingPolicy#code
        '''
        result = self._values.get("code")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def ttl(self) -> typing.Optional[jsii.Number]:
        '''The TTL (in seconds) for which to cache responses with the corresponding status code.

        The maximum allowed value is 1800s (30 minutes), noting that infrequently accessed objects may be evicted from the cache before the defined TTL.

        :schema: BackendConfigSpecCdnNegativeCachingPolicy#ttl
        '''
        result = self._values.get("ttl")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCdnNegativeCachingPolicy(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCdnSignedUrlKeys",
    jsii_struct_bases=[],
    name_mapping={
        "key_name": "keyName",
        "key_value": "keyValue",
        "secret_name": "secretName",
    },
)
class BackendConfigSpecCdnSignedUrlKeys:
    def __init__(
        self,
        *,
        key_name: typing.Optional[builtins.str] = None,
        key_value: typing.Optional[builtins.str] = None,
        secret_name: typing.Optional[builtins.str] = None,
    ) -> None:
        '''SignedUrlKey represents a customer-supplied Signing Key used by Cloud CDN Signed URLs.

        :param key_name: KeyName: Name of the key. The name must be 1-63 characters long, and comply with RFC1035. Specifically, the name must be 1-63 characters long and match the regular expression ``[a-z]([-a-z0-9]*[a-z0-9])?`` which means the first character must be a lowercase letter, and all following characters must be a dash, lowercase letter, or digit, except the last character, which cannot be a dash.
        :param key_value: KeyValue: 128-bit key value used for signing the URL. The key value must be a valid RFC 4648 Section 5 base64url encoded string.
        :param secret_name: The name of a k8s secret which stores the 128-bit key value used for signing the URL. The key value must be a valid RFC 4648 Section 5 base64url encoded string

        :schema: BackendConfigSpecCdnSignedUrlKeys
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__c2bfc9b5d277ae2443b64ec9704ba599608ab4b41c297adb864d61ad89882fad)
            check_type(argname="argument key_name", value=key_name, expected_type=type_hints["key_name"])
            check_type(argname="argument key_value", value=key_value, expected_type=type_hints["key_value"])
            check_type(argname="argument secret_name", value=secret_name, expected_type=type_hints["secret_name"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if key_name is not None:
            self._values["key_name"] = key_name
        if key_value is not None:
            self._values["key_value"] = key_value
        if secret_name is not None:
            self._values["secret_name"] = secret_name

    @builtins.property
    def key_name(self) -> typing.Optional[builtins.str]:
        '''KeyName: Name of the key.

        The name must be 1-63 characters long, and comply with RFC1035. Specifically, the name must be 1-63 characters long and match the regular expression ``[a-z]([-a-z0-9]*[a-z0-9])?`` which means the first character must be a lowercase letter, and all following characters must be a dash, lowercase letter, or digit, except the last character, which cannot be a dash.

        :schema: BackendConfigSpecCdnSignedUrlKeys#keyName
        '''
        result = self._values.get("key_name")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def key_value(self) -> typing.Optional[builtins.str]:
        '''KeyValue: 128-bit key value used for signing the URL.

        The key value must be a valid RFC 4648 Section 5 base64url encoded string.

        :schema: BackendConfigSpecCdnSignedUrlKeys#keyValue
        '''
        result = self._values.get("key_value")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def secret_name(self) -> typing.Optional[builtins.str]:
        '''The name of a k8s secret which stores the 128-bit key value used for signing the URL.

        The key value must be a valid RFC 4648 Section 5 base64url encoded string

        :schema: BackendConfigSpecCdnSignedUrlKeys#secretName
        '''
        result = self._values.get("secret_name")
        return typing.cast(typing.Optional[builtins.str], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCdnSignedUrlKeys(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecConnectionDraining",
    jsii_struct_bases=[],
    name_mapping={"draining_timeout_sec": "drainingTimeoutSec"},
)
class BackendConfigSpecConnectionDraining:
    def __init__(
        self,
        *,
        draining_timeout_sec: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''ConnectionDrainingConfig contains configuration for connection draining.

        For now the draining timeout. May manage more settings in the future.

        :param draining_timeout_sec: Draining timeout in seconds.

        :schema: BackendConfigSpecConnectionDraining
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__a8a4d781a8209b710ae791e501ffc537c4e1abe71291f6bfe94b802fde855fc0)
            check_type(argname="argument draining_timeout_sec", value=draining_timeout_sec, expected_type=type_hints["draining_timeout_sec"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if draining_timeout_sec is not None:
            self._values["draining_timeout_sec"] = draining_timeout_sec

    @builtins.property
    def draining_timeout_sec(self) -> typing.Optional[jsii.Number]:
        '''Draining timeout in seconds.

        :schema: BackendConfigSpecConnectionDraining#drainingTimeoutSec
        '''
        result = self._values.get("draining_timeout_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecConnectionDraining(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCustomRequestHeaders",
    jsii_struct_bases=[],
    name_mapping={"headers": "headers"},
)
class BackendConfigSpecCustomRequestHeaders:
    def __init__(
        self,
        *,
        headers: typing.Optional[typing.Sequence[builtins.str]] = None,
    ) -> None:
        '''CustomRequestHeadersConfig contains configuration for custom request headers.

        :param headers: 

        :schema: BackendConfigSpecCustomRequestHeaders
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__8dc5d6b2f96616a24acd3ccf5a6df73b067f9250a84a34ae8d3eb2d6996a106d)
            check_type(argname="argument headers", value=headers, expected_type=type_hints["headers"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if headers is not None:
            self._values["headers"] = headers

    @builtins.property
    def headers(self) -> typing.Optional[typing.List[builtins.str]]:
        '''
        :schema: BackendConfigSpecCustomRequestHeaders#headers
        '''
        result = self._values.get("headers")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCustomRequestHeaders(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecCustomResponseHeaders",
    jsii_struct_bases=[],
    name_mapping={"headers": "headers"},
)
class BackendConfigSpecCustomResponseHeaders:
    def __init__(
        self,
        *,
        headers: typing.Optional[typing.Sequence[builtins.str]] = None,
    ) -> None:
        '''CustomResponseHeadersConfig contains configuration for custom response headers.

        :param headers: 

        :schema: BackendConfigSpecCustomResponseHeaders
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__3d55a2d2ade28e64a421578946506b571eb485866dc843b5f94b163bf1b36867)
            check_type(argname="argument headers", value=headers, expected_type=type_hints["headers"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if headers is not None:
            self._values["headers"] = headers

    @builtins.property
    def headers(self) -> typing.Optional[typing.List[builtins.str]]:
        '''
        :schema: BackendConfigSpecCustomResponseHeaders#headers
        '''
        result = self._values.get("headers")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecCustomResponseHeaders(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecHealthCheck",
    jsii_struct_bases=[],
    name_mapping={
        "check_interval_sec": "checkIntervalSec",
        "healthy_threshold": "healthyThreshold",
        "port": "port",
        "request_path": "requestPath",
        "timeout_sec": "timeoutSec",
        "type": "type",
        "unhealthy_threshold": "unhealthyThreshold",
    },
)
class BackendConfigSpecHealthCheck:
    def __init__(
        self,
        *,
        check_interval_sec: typing.Optional[jsii.Number] = None,
        healthy_threshold: typing.Optional[jsii.Number] = None,
        port: typing.Optional[jsii.Number] = None,
        request_path: typing.Optional[builtins.str] = None,
        timeout_sec: typing.Optional[jsii.Number] = None,
        type: typing.Optional[builtins.str] = None,
        unhealthy_threshold: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''HealthCheckConfig contains configuration for the health check.

        :param check_interval_sec: CheckIntervalSec is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param healthy_threshold: HealthyThreshold is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param port: Port is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks. If Port is used, the controller updates portSpecification as well
        :param request_path: RequestPath is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param timeout_sec: TimeoutSec is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param type: Type is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param unhealthy_threshold: UnhealthyThreshold is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__79c151f1f31cafead3943aeaed93e335f4e87ceb8da1e792180414251cfe761a)
            check_type(argname="argument check_interval_sec", value=check_interval_sec, expected_type=type_hints["check_interval_sec"])
            check_type(argname="argument healthy_threshold", value=healthy_threshold, expected_type=type_hints["healthy_threshold"])
            check_type(argname="argument port", value=port, expected_type=type_hints["port"])
            check_type(argname="argument request_path", value=request_path, expected_type=type_hints["request_path"])
            check_type(argname="argument timeout_sec", value=timeout_sec, expected_type=type_hints["timeout_sec"])
            check_type(argname="argument type", value=type, expected_type=type_hints["type"])
            check_type(argname="argument unhealthy_threshold", value=unhealthy_threshold, expected_type=type_hints["unhealthy_threshold"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if check_interval_sec is not None:
            self._values["check_interval_sec"] = check_interval_sec
        if healthy_threshold is not None:
            self._values["healthy_threshold"] = healthy_threshold
        if port is not None:
            self._values["port"] = port
        if request_path is not None:
            self._values["request_path"] = request_path
        if timeout_sec is not None:
            self._values["timeout_sec"] = timeout_sec
        if type is not None:
            self._values["type"] = type
        if unhealthy_threshold is not None:
            self._values["unhealthy_threshold"] = unhealthy_threshold

    @builtins.property
    def check_interval_sec(self) -> typing.Optional[jsii.Number]:
        '''CheckIntervalSec is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck#checkIntervalSec
        '''
        result = self._values.get("check_interval_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def healthy_threshold(self) -> typing.Optional[jsii.Number]:
        '''HealthyThreshold is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck#healthyThreshold
        '''
        result = self._values.get("healthy_threshold")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def port(self) -> typing.Optional[jsii.Number]:
        '''Port is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks. If Port is used, the controller updates portSpecification as well

        :schema: BackendConfigSpecHealthCheck#port
        '''
        result = self._values.get("port")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def request_path(self) -> typing.Optional[builtins.str]:
        '''RequestPath is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck#requestPath
        '''
        result = self._values.get("request_path")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def timeout_sec(self) -> typing.Optional[jsii.Number]:
        '''TimeoutSec is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck#timeoutSec
        '''
        result = self._values.get("timeout_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def type(self) -> typing.Optional[builtins.str]:
        '''Type is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck#type
        '''
        result = self._values.get("type")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def unhealthy_threshold(self) -> typing.Optional[jsii.Number]:
        '''UnhealthyThreshold is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigSpecHealthCheck#unhealthyThreshold
        '''
        result = self._values.get("unhealthy_threshold")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecHealthCheck(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecIap",
    jsii_struct_bases=[],
    name_mapping={
        "enabled": "enabled",
        "oauthclient_credentials": "oauthclientCredentials",
    },
)
class BackendConfigSpecIap:
    def __init__(
        self,
        *,
        enabled: builtins.bool,
        oauthclient_credentials: typing.Optional[typing.Union["BackendConfigSpecIapOauthclientCredentials", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''IAPConfig contains configuration for IAP-enabled backends.

        :param enabled: 
        :param oauthclient_credentials: OAuthClientCredentials contains credentials for a single IAP-enabled backend.

        :schema: BackendConfigSpecIap
        '''
        if isinstance(oauthclient_credentials, dict):
            oauthclient_credentials = BackendConfigSpecIapOauthclientCredentials(**oauthclient_credentials)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__13c7a07f21084397e395ce02eb15e5467ad5a76b4b555f0d12b8d64cd9eb3a84)
            check_type(argname="argument enabled", value=enabled, expected_type=type_hints["enabled"])
            check_type(argname="argument oauthclient_credentials", value=oauthclient_credentials, expected_type=type_hints["oauthclient_credentials"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "enabled": enabled,
        }
        if oauthclient_credentials is not None:
            self._values["oauthclient_credentials"] = oauthclient_credentials

    @builtins.property
    def enabled(self) -> builtins.bool:
        '''
        :schema: BackendConfigSpecIap#enabled
        '''
        result = self._values.get("enabled")
        assert result is not None, "Required property 'enabled' is missing"
        return typing.cast(builtins.bool, result)

    @builtins.property
    def oauthclient_credentials(
        self,
    ) -> typing.Optional["BackendConfigSpecIapOauthclientCredentials"]:
        '''OAuthClientCredentials contains credentials for a single IAP-enabled backend.

        :schema: BackendConfigSpecIap#oauthclientCredentials
        '''
        result = self._values.get("oauthclient_credentials")
        return typing.cast(typing.Optional["BackendConfigSpecIapOauthclientCredentials"], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecIap(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecIapOauthclientCredentials",
    jsii_struct_bases=[],
    name_mapping={
        "secret_name": "secretName",
        "client_id": "clientId",
        "client_secret": "clientSecret",
    },
)
class BackendConfigSpecIapOauthclientCredentials:
    def __init__(
        self,
        *,
        secret_name: builtins.str,
        client_id: typing.Optional[builtins.str] = None,
        client_secret: typing.Optional[builtins.str] = None,
    ) -> None:
        '''OAuthClientCredentials contains credentials for a single IAP-enabled backend.

        :param secret_name: The name of a k8s secret which stores the OAuth client id & secret.
        :param client_id: Direct reference to OAuth client id.
        :param client_secret: Direct reference to OAuth client secret.

        :schema: BackendConfigSpecIapOauthclientCredentials
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__9ce01ce36f37e0da11c74ffa5ee924df4b65d1e3adbf2ea4b16241b79f60c21e)
            check_type(argname="argument secret_name", value=secret_name, expected_type=type_hints["secret_name"])
            check_type(argname="argument client_id", value=client_id, expected_type=type_hints["client_id"])
            check_type(argname="argument client_secret", value=client_secret, expected_type=type_hints["client_secret"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "secret_name": secret_name,
        }
        if client_id is not None:
            self._values["client_id"] = client_id
        if client_secret is not None:
            self._values["client_secret"] = client_secret

    @builtins.property
    def secret_name(self) -> builtins.str:
        '''The name of a k8s secret which stores the OAuth client id & secret.

        :schema: BackendConfigSpecIapOauthclientCredentials#secretName
        '''
        result = self._values.get("secret_name")
        assert result is not None, "Required property 'secret_name' is missing"
        return typing.cast(builtins.str, result)

    @builtins.property
    def client_id(self) -> typing.Optional[builtins.str]:
        '''Direct reference to OAuth client id.

        :schema: BackendConfigSpecIapOauthclientCredentials#clientID
        '''
        result = self._values.get("client_id")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def client_secret(self) -> typing.Optional[builtins.str]:
        '''Direct reference to OAuth client secret.

        :schema: BackendConfigSpecIapOauthclientCredentials#clientSecret
        '''
        result = self._values.get("client_secret")
        return typing.cast(typing.Optional[builtins.str], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecIapOauthclientCredentials(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecLogging",
    jsii_struct_bases=[],
    name_mapping={"enable": "enable", "sample_rate": "sampleRate"},
)
class BackendConfigSpecLogging:
    def __init__(
        self,
        *,
        enable: typing.Optional[builtins.bool] = None,
        sample_rate: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''LogConfig contains configuration for logging.

        :param enable: This field denotes whether to enable logging for the load balancer traffic served by this backend service.
        :param sample_rate: This field can only be specified if logging is enabled for this backend service. The value of the field must be in [0, 1]. This configures the sampling rate of requests to the load balancer where 1.0 means all logged requests are reported and 0.0 means no logged requests are reported. The default value is 1.0.

        :schema: BackendConfigSpecLogging
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__70de348aa7cf778b7313e13ebebbfdd0982a935c020de62a80311862fc6623a9)
            check_type(argname="argument enable", value=enable, expected_type=type_hints["enable"])
            check_type(argname="argument sample_rate", value=sample_rate, expected_type=type_hints["sample_rate"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if enable is not None:
            self._values["enable"] = enable
        if sample_rate is not None:
            self._values["sample_rate"] = sample_rate

    @builtins.property
    def enable(self) -> typing.Optional[builtins.bool]:
        '''This field denotes whether to enable logging for the load balancer traffic served by this backend service.

        :schema: BackendConfigSpecLogging#enable
        '''
        result = self._values.get("enable")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def sample_rate(self) -> typing.Optional[jsii.Number]:
        '''This field can only be specified if logging is enabled for this backend service.

        The value of the field must be in [0, 1]. This configures the sampling rate of requests to the load balancer where 1.0 means all logged requests are reported and 0.0 means no logged requests are reported. The default value is 1.0.

        :schema: BackendConfigSpecLogging#sampleRate
        '''
        result = self._values.get("sample_rate")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecLogging(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecSecurityPolicy",
    jsii_struct_bases=[],
    name_mapping={"name": "name"},
)
class BackendConfigSpecSecurityPolicy:
    def __init__(self, *, name: builtins.str) -> None:
        '''SecurityPolicyConfig contains configuration for CloudArmor-enabled backends.

        If not specified, the controller will not reconcile the security policy configuration. In other words, users can make changes in GCE without the controller overwriting them.

        :param name: Name of the security policy that should be associated. If set to empty, the existing security policy on the backend will be removed.

        :schema: BackendConfigSpecSecurityPolicy
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__98ab3b68c0a293fb2cbc562276ebfc79835049626e8b0cd436f56437f7ef2aad)
            check_type(argname="argument name", value=name, expected_type=type_hints["name"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "name": name,
        }

    @builtins.property
    def name(self) -> builtins.str:
        '''Name of the security policy that should be associated.

        If set to empty, the existing security policy on the backend will be removed.

        :schema: BackendConfigSpecSecurityPolicy#name
        '''
        result = self._values.get("name")
        assert result is not None, "Required property 'name' is missing"
        return typing.cast(builtins.str, result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecSecurityPolicy(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigSpecSessionAffinity",
    jsii_struct_bases=[],
    name_mapping={
        "affinity_cookie_ttl_sec": "affinityCookieTtlSec",
        "affinity_type": "affinityType",
    },
)
class BackendConfigSpecSessionAffinity:
    def __init__(
        self,
        *,
        affinity_cookie_ttl_sec: typing.Optional[jsii.Number] = None,
        affinity_type: typing.Optional[builtins.str] = None,
    ) -> None:
        '''SessionAffinityConfig contains configuration for stickiness parameters.

        :param affinity_cookie_ttl_sec: 
        :param affinity_type: 

        :schema: BackendConfigSpecSessionAffinity
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__12b36eeaa8e78922a88283ed58641b01eb9642275277cb5d79c87068d3e196b8)
            check_type(argname="argument affinity_cookie_ttl_sec", value=affinity_cookie_ttl_sec, expected_type=type_hints["affinity_cookie_ttl_sec"])
            check_type(argname="argument affinity_type", value=affinity_type, expected_type=type_hints["affinity_type"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if affinity_cookie_ttl_sec is not None:
            self._values["affinity_cookie_ttl_sec"] = affinity_cookie_ttl_sec
        if affinity_type is not None:
            self._values["affinity_type"] = affinity_type

    @builtins.property
    def affinity_cookie_ttl_sec(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigSpecSessionAffinity#affinityCookieTtlSec
        '''
        result = self._values.get("affinity_cookie_ttl_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def affinity_type(self) -> typing.Optional[builtins.str]:
        '''
        :schema: BackendConfigSpecSessionAffinity#affinityType
        '''
        result = self._values.get("affinity_type")
        return typing.cast(typing.Optional[builtins.str], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigSpecSessionAffinity(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


class BackendConfigV1Beta1(
    _cdk8s_d3d9af27.ApiObject,
    metaclass=jsii.JSIIMeta,
    jsii_type="comgooglecloud.BackendConfigV1Beta1",
):
    '''
    :schema: BackendConfigV1Beta1
    '''

    def __init__(
        self,
        scope: _constructs_77d1e7e8.Construct,
        id: builtins.str,
        *,
        metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
        spec: typing.Optional[typing.Union["BackendConfigV1Beta1Spec", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''Defines a "BackendConfigV1Beta1" API object.

        :param scope: the scope in which to define this object.
        :param id: a scope-local name for the object.
        :param metadata: 
        :param spec: BackendConfigSpec is the spec for a BackendConfig resource.
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__344b0717d420cfea1a675b9b85d42abd06963df4f4e2b6703b26b6c888941945)
            check_type(argname="argument scope", value=scope, expected_type=type_hints["scope"])
            check_type(argname="argument id", value=id, expected_type=type_hints["id"])
        props = BackendConfigV1Beta1Props(metadata=metadata, spec=spec)

        jsii.create(self.__class__, self, [scope, id, props])

    @jsii.member(jsii_name="manifest")
    @builtins.classmethod
    def manifest(
        cls,
        *,
        metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
        spec: typing.Optional[typing.Union["BackendConfigV1Beta1Spec", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> typing.Any:
        '''Renders a Kubernetes manifest for "BackendConfigV1Beta1".

        This can be used to inline resource manifests inside other objects (e.g. as templates).

        :param metadata: 
        :param spec: BackendConfigSpec is the spec for a BackendConfig resource.
        '''
        props = BackendConfigV1Beta1Props(metadata=metadata, spec=spec)

        return typing.cast(typing.Any, jsii.sinvoke(cls, "manifest", [props]))

    @jsii.member(jsii_name="toJson")
    def to_json(self) -> typing.Any:
        '''Renders the object to Kubernetes JSON.'''
        return typing.cast(typing.Any, jsii.invoke(self, "toJson", []))

    @jsii.python.classproperty
    @jsii.member(jsii_name="GVK")
    def GVK(cls) -> _cdk8s_d3d9af27.GroupVersionKind:
        '''Returns the apiVersion and kind for "BackendConfigV1Beta1".'''
        return typing.cast(_cdk8s_d3d9af27.GroupVersionKind, jsii.sget(cls, "GVK"))


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1Props",
    jsii_struct_bases=[],
    name_mapping={"metadata": "metadata", "spec": "spec"},
)
class BackendConfigV1Beta1Props:
    def __init__(
        self,
        *,
        metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
        spec: typing.Optional[typing.Union["BackendConfigV1Beta1Spec", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''
        :param metadata: 
        :param spec: BackendConfigSpec is the spec for a BackendConfig resource.

        :schema: BackendConfigV1Beta1
        '''
        if isinstance(metadata, dict):
            metadata = _cdk8s_d3d9af27.ApiObjectMetadata(**metadata)
        if isinstance(spec, dict):
            spec = BackendConfigV1Beta1Spec(**spec)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__f97dd20787fe9d41b4c4414f7cf16853f3f154133fed2b3101de407e38a242f6)
            check_type(argname="argument metadata", value=metadata, expected_type=type_hints["metadata"])
            check_type(argname="argument spec", value=spec, expected_type=type_hints["spec"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if metadata is not None:
            self._values["metadata"] = metadata
        if spec is not None:
            self._values["spec"] = spec

    @builtins.property
    def metadata(self) -> typing.Optional[_cdk8s_d3d9af27.ApiObjectMetadata]:
        '''
        :schema: BackendConfigV1Beta1#metadata
        '''
        result = self._values.get("metadata")
        return typing.cast(typing.Optional[_cdk8s_d3d9af27.ApiObjectMetadata], result)

    @builtins.property
    def spec(self) -> typing.Optional["BackendConfigV1Beta1Spec"]:
        '''BackendConfigSpec is the spec for a BackendConfig resource.

        :schema: BackendConfigV1Beta1#spec
        '''
        result = self._values.get("spec")
        return typing.cast(typing.Optional["BackendConfigV1Beta1Spec"], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1Props(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1Spec",
    jsii_struct_bases=[],
    name_mapping={
        "cdn": "cdn",
        "connection_draining": "connectionDraining",
        "custom_request_headers": "customRequestHeaders",
        "health_check": "healthCheck",
        "iap": "iap",
        "security_policy": "securityPolicy",
        "session_affinity": "sessionAffinity",
        "timeout_sec": "timeoutSec",
    },
)
class BackendConfigV1Beta1Spec:
    def __init__(
        self,
        *,
        cdn: typing.Optional[typing.Union["BackendConfigV1Beta1SpecCdn", typing.Dict[builtins.str, typing.Any]]] = None,
        connection_draining: typing.Optional[typing.Union["BackendConfigV1Beta1SpecConnectionDraining", typing.Dict[builtins.str, typing.Any]]] = None,
        custom_request_headers: typing.Optional[typing.Union["BackendConfigV1Beta1SpecCustomRequestHeaders", typing.Dict[builtins.str, typing.Any]]] = None,
        health_check: typing.Optional[typing.Union["BackendConfigV1Beta1SpecHealthCheck", typing.Dict[builtins.str, typing.Any]]] = None,
        iap: typing.Optional[typing.Union["BackendConfigV1Beta1SpecIap", typing.Dict[builtins.str, typing.Any]]] = None,
        security_policy: typing.Optional[typing.Union["BackendConfigV1Beta1SpecSecurityPolicy", typing.Dict[builtins.str, typing.Any]]] = None,
        session_affinity: typing.Optional[typing.Union["BackendConfigV1Beta1SpecSessionAffinity", typing.Dict[builtins.str, typing.Any]]] = None,
        timeout_sec: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''BackendConfigSpec is the spec for a BackendConfig resource.

        :param cdn: CDNConfig contains configuration for CDN-enabled backends.
        :param connection_draining: ConnectionDrainingConfig contains configuration for connection draining. For now the draining timeout. May manage more settings in the future.
        :param custom_request_headers: CustomRequestHeadersConfig contains configuration for custom request headers.
        :param health_check: HealthCheckConfig contains configuration for the health check.
        :param iap: IAPConfig contains configuration for IAP-enabled backends.
        :param security_policy: SecurityPolicyConfig contains configuration for CloudArmor-enabled backends. If not specified, the controller will not reconcile the security policy configuration. In other words, users can make changes in GCE without the controller overwriting them.
        :param session_affinity: SessionAffinityConfig contains configuration for stickiness parameters.
        :param timeout_sec: 

        :schema: BackendConfigV1Beta1Spec
        '''
        if isinstance(cdn, dict):
            cdn = BackendConfigV1Beta1SpecCdn(**cdn)
        if isinstance(connection_draining, dict):
            connection_draining = BackendConfigV1Beta1SpecConnectionDraining(**connection_draining)
        if isinstance(custom_request_headers, dict):
            custom_request_headers = BackendConfigV1Beta1SpecCustomRequestHeaders(**custom_request_headers)
        if isinstance(health_check, dict):
            health_check = BackendConfigV1Beta1SpecHealthCheck(**health_check)
        if isinstance(iap, dict):
            iap = BackendConfigV1Beta1SpecIap(**iap)
        if isinstance(security_policy, dict):
            security_policy = BackendConfigV1Beta1SpecSecurityPolicy(**security_policy)
        if isinstance(session_affinity, dict):
            session_affinity = BackendConfigV1Beta1SpecSessionAffinity(**session_affinity)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__ce76bcc047783b5343fff4e7a4014ad78591646cde2f19b57180e83c0addac84)
            check_type(argname="argument cdn", value=cdn, expected_type=type_hints["cdn"])
            check_type(argname="argument connection_draining", value=connection_draining, expected_type=type_hints["connection_draining"])
            check_type(argname="argument custom_request_headers", value=custom_request_headers, expected_type=type_hints["custom_request_headers"])
            check_type(argname="argument health_check", value=health_check, expected_type=type_hints["health_check"])
            check_type(argname="argument iap", value=iap, expected_type=type_hints["iap"])
            check_type(argname="argument security_policy", value=security_policy, expected_type=type_hints["security_policy"])
            check_type(argname="argument session_affinity", value=session_affinity, expected_type=type_hints["session_affinity"])
            check_type(argname="argument timeout_sec", value=timeout_sec, expected_type=type_hints["timeout_sec"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if cdn is not None:
            self._values["cdn"] = cdn
        if connection_draining is not None:
            self._values["connection_draining"] = connection_draining
        if custom_request_headers is not None:
            self._values["custom_request_headers"] = custom_request_headers
        if health_check is not None:
            self._values["health_check"] = health_check
        if iap is not None:
            self._values["iap"] = iap
        if security_policy is not None:
            self._values["security_policy"] = security_policy
        if session_affinity is not None:
            self._values["session_affinity"] = session_affinity
        if timeout_sec is not None:
            self._values["timeout_sec"] = timeout_sec

    @builtins.property
    def cdn(self) -> typing.Optional["BackendConfigV1Beta1SpecCdn"]:
        '''CDNConfig contains configuration for CDN-enabled backends.

        :schema: BackendConfigV1Beta1Spec#cdn
        '''
        result = self._values.get("cdn")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecCdn"], result)

    @builtins.property
    def connection_draining(
        self,
    ) -> typing.Optional["BackendConfigV1Beta1SpecConnectionDraining"]:
        '''ConnectionDrainingConfig contains configuration for connection draining.

        For now the draining timeout. May manage more settings in the future.

        :schema: BackendConfigV1Beta1Spec#connectionDraining
        '''
        result = self._values.get("connection_draining")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecConnectionDraining"], result)

    @builtins.property
    def custom_request_headers(
        self,
    ) -> typing.Optional["BackendConfigV1Beta1SpecCustomRequestHeaders"]:
        '''CustomRequestHeadersConfig contains configuration for custom request headers.

        :schema: BackendConfigV1Beta1Spec#customRequestHeaders
        '''
        result = self._values.get("custom_request_headers")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecCustomRequestHeaders"], result)

    @builtins.property
    def health_check(self) -> typing.Optional["BackendConfigV1Beta1SpecHealthCheck"]:
        '''HealthCheckConfig contains configuration for the health check.

        :schema: BackendConfigV1Beta1Spec#healthCheck
        '''
        result = self._values.get("health_check")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecHealthCheck"], result)

    @builtins.property
    def iap(self) -> typing.Optional["BackendConfigV1Beta1SpecIap"]:
        '''IAPConfig contains configuration for IAP-enabled backends.

        :schema: BackendConfigV1Beta1Spec#iap
        '''
        result = self._values.get("iap")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecIap"], result)

    @builtins.property
    def security_policy(
        self,
    ) -> typing.Optional["BackendConfigV1Beta1SpecSecurityPolicy"]:
        '''SecurityPolicyConfig contains configuration for CloudArmor-enabled backends.

        If not specified, the controller will not reconcile the security policy configuration. In other words, users can make changes in GCE without the controller overwriting them.

        :schema: BackendConfigV1Beta1Spec#securityPolicy
        '''
        result = self._values.get("security_policy")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecSecurityPolicy"], result)

    @builtins.property
    def session_affinity(
        self,
    ) -> typing.Optional["BackendConfigV1Beta1SpecSessionAffinity"]:
        '''SessionAffinityConfig contains configuration for stickiness parameters.

        :schema: BackendConfigV1Beta1Spec#sessionAffinity
        '''
        result = self._values.get("session_affinity")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecSessionAffinity"], result)

    @builtins.property
    def timeout_sec(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigV1Beta1Spec#timeoutSec
        '''
        result = self._values.get("timeout_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1Spec(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecCdn",
    jsii_struct_bases=[],
    name_mapping={"enabled": "enabled", "cache_policy": "cachePolicy"},
)
class BackendConfigV1Beta1SpecCdn:
    def __init__(
        self,
        *,
        enabled: builtins.bool,
        cache_policy: typing.Optional[typing.Union["BackendConfigV1Beta1SpecCdnCachePolicy", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''CDNConfig contains configuration for CDN-enabled backends.

        :param enabled: 
        :param cache_policy: CacheKeyPolicy contains configuration for how requests to a CDN-enabled backend are cached.

        :schema: BackendConfigV1Beta1SpecCdn
        '''
        if isinstance(cache_policy, dict):
            cache_policy = BackendConfigV1Beta1SpecCdnCachePolicy(**cache_policy)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__7356f4b96f97aa57f7f1d1e7d1ee04d987bafe3588ef34aee1d27a535380fb82)
            check_type(argname="argument enabled", value=enabled, expected_type=type_hints["enabled"])
            check_type(argname="argument cache_policy", value=cache_policy, expected_type=type_hints["cache_policy"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "enabled": enabled,
        }
        if cache_policy is not None:
            self._values["cache_policy"] = cache_policy

    @builtins.property
    def enabled(self) -> builtins.bool:
        '''
        :schema: BackendConfigV1Beta1SpecCdn#enabled
        '''
        result = self._values.get("enabled")
        assert result is not None, "Required property 'enabled' is missing"
        return typing.cast(builtins.bool, result)

    @builtins.property
    def cache_policy(self) -> typing.Optional["BackendConfigV1Beta1SpecCdnCachePolicy"]:
        '''CacheKeyPolicy contains configuration for how requests to a CDN-enabled backend are cached.

        :schema: BackendConfigV1Beta1SpecCdn#cachePolicy
        '''
        result = self._values.get("cache_policy")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecCdnCachePolicy"], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecCdn(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecCdnCachePolicy",
    jsii_struct_bases=[],
    name_mapping={
        "include_host": "includeHost",
        "include_protocol": "includeProtocol",
        "include_query_string": "includeQueryString",
        "query_string_blacklist": "queryStringBlacklist",
        "query_string_whitelist": "queryStringWhitelist",
    },
)
class BackendConfigV1Beta1SpecCdnCachePolicy:
    def __init__(
        self,
        *,
        include_host: typing.Optional[builtins.bool] = None,
        include_protocol: typing.Optional[builtins.bool] = None,
        include_query_string: typing.Optional[builtins.bool] = None,
        query_string_blacklist: typing.Optional[typing.Sequence[builtins.str]] = None,
        query_string_whitelist: typing.Optional[typing.Sequence[builtins.str]] = None,
    ) -> None:
        '''CacheKeyPolicy contains configuration for how requests to a CDN-enabled backend are cached.

        :param include_host: If true, requests to different hosts will be cached separately.
        :param include_protocol: If true, http and https requests will be cached separately.
        :param include_query_string: If true, query string parameters are included in the cache key according to QueryStringBlacklist and QueryStringWhitelist. If neither is set, the entire query string is included and if false the entire query string is excluded.
        :param query_string_blacklist: Names of query strint parameters to exclude from cache keys. All other parameters are included. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.
        :param query_string_whitelist: Names of query string parameters to include in cache keys. All other parameters are excluded. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.

        :schema: BackendConfigV1Beta1SpecCdnCachePolicy
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__f3de908e6a83c0f93d3ad698e79d4a79b4f86b58a6f774bedbd980fbdf53e06d)
            check_type(argname="argument include_host", value=include_host, expected_type=type_hints["include_host"])
            check_type(argname="argument include_protocol", value=include_protocol, expected_type=type_hints["include_protocol"])
            check_type(argname="argument include_query_string", value=include_query_string, expected_type=type_hints["include_query_string"])
            check_type(argname="argument query_string_blacklist", value=query_string_blacklist, expected_type=type_hints["query_string_blacklist"])
            check_type(argname="argument query_string_whitelist", value=query_string_whitelist, expected_type=type_hints["query_string_whitelist"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if include_host is not None:
            self._values["include_host"] = include_host
        if include_protocol is not None:
            self._values["include_protocol"] = include_protocol
        if include_query_string is not None:
            self._values["include_query_string"] = include_query_string
        if query_string_blacklist is not None:
            self._values["query_string_blacklist"] = query_string_blacklist
        if query_string_whitelist is not None:
            self._values["query_string_whitelist"] = query_string_whitelist

    @builtins.property
    def include_host(self) -> typing.Optional[builtins.bool]:
        '''If true, requests to different hosts will be cached separately.

        :schema: BackendConfigV1Beta1SpecCdnCachePolicy#includeHost
        '''
        result = self._values.get("include_host")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def include_protocol(self) -> typing.Optional[builtins.bool]:
        '''If true, http and https requests will be cached separately.

        :schema: BackendConfigV1Beta1SpecCdnCachePolicy#includeProtocol
        '''
        result = self._values.get("include_protocol")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def include_query_string(self) -> typing.Optional[builtins.bool]:
        '''If true, query string parameters are included in the cache key according to QueryStringBlacklist and QueryStringWhitelist.

        If neither is set, the entire query string is included and if false the entire query string is excluded.

        :schema: BackendConfigV1Beta1SpecCdnCachePolicy#includeQueryString
        '''
        result = self._values.get("include_query_string")
        return typing.cast(typing.Optional[builtins.bool], result)

    @builtins.property
    def query_string_blacklist(self) -> typing.Optional[typing.List[builtins.str]]:
        '''Names of query strint parameters to exclude from cache keys.

        All other parameters are included. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.

        :schema: BackendConfigV1Beta1SpecCdnCachePolicy#queryStringBlacklist
        '''
        result = self._values.get("query_string_blacklist")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    @builtins.property
    def query_string_whitelist(self) -> typing.Optional[typing.List[builtins.str]]:
        '''Names of query string parameters to include in cache keys.

        All other parameters are excluded. Either specify QueryStringBlacklist or QueryStringWhitelist, but not both.

        :schema: BackendConfigV1Beta1SpecCdnCachePolicy#queryStringWhitelist
        '''
        result = self._values.get("query_string_whitelist")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecCdnCachePolicy(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecConnectionDraining",
    jsii_struct_bases=[],
    name_mapping={"draining_timeout_sec": "drainingTimeoutSec"},
)
class BackendConfigV1Beta1SpecConnectionDraining:
    def __init__(
        self,
        *,
        draining_timeout_sec: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''ConnectionDrainingConfig contains configuration for connection draining.

        For now the draining timeout. May manage more settings in the future.

        :param draining_timeout_sec: Draining timeout in seconds.

        :schema: BackendConfigV1Beta1SpecConnectionDraining
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__c6f2fcacdc49b8b8096a586cf8c3e9c44397b88e90594778bad466213d4d3f38)
            check_type(argname="argument draining_timeout_sec", value=draining_timeout_sec, expected_type=type_hints["draining_timeout_sec"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if draining_timeout_sec is not None:
            self._values["draining_timeout_sec"] = draining_timeout_sec

    @builtins.property
    def draining_timeout_sec(self) -> typing.Optional[jsii.Number]:
        '''Draining timeout in seconds.

        :schema: BackendConfigV1Beta1SpecConnectionDraining#drainingTimeoutSec
        '''
        result = self._values.get("draining_timeout_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecConnectionDraining(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecCustomRequestHeaders",
    jsii_struct_bases=[],
    name_mapping={"headers": "headers"},
)
class BackendConfigV1Beta1SpecCustomRequestHeaders:
    def __init__(
        self,
        *,
        headers: typing.Optional[typing.Sequence[builtins.str]] = None,
    ) -> None:
        '''CustomRequestHeadersConfig contains configuration for custom request headers.

        :param headers: 

        :schema: BackendConfigV1Beta1SpecCustomRequestHeaders
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__778f47b4768dec1ed1170f49b440363a1f9082599e4b897535aa179f840e7c2e)
            check_type(argname="argument headers", value=headers, expected_type=type_hints["headers"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if headers is not None:
            self._values["headers"] = headers

    @builtins.property
    def headers(self) -> typing.Optional[typing.List[builtins.str]]:
        '''
        :schema: BackendConfigV1Beta1SpecCustomRequestHeaders#headers
        '''
        result = self._values.get("headers")
        return typing.cast(typing.Optional[typing.List[builtins.str]], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecCustomRequestHeaders(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecHealthCheck",
    jsii_struct_bases=[],
    name_mapping={
        "check_interval_sec": "checkIntervalSec",
        "healthy_threshold": "healthyThreshold",
        "port": "port",
        "request_path": "requestPath",
        "timeout_sec": "timeoutSec",
        "type": "type",
        "unhealthy_threshold": "unhealthyThreshold",
    },
)
class BackendConfigV1Beta1SpecHealthCheck:
    def __init__(
        self,
        *,
        check_interval_sec: typing.Optional[jsii.Number] = None,
        healthy_threshold: typing.Optional[jsii.Number] = None,
        port: typing.Optional[jsii.Number] = None,
        request_path: typing.Optional[builtins.str] = None,
        timeout_sec: typing.Optional[jsii.Number] = None,
        type: typing.Optional[builtins.str] = None,
        unhealthy_threshold: typing.Optional[jsii.Number] = None,
    ) -> None:
        '''HealthCheckConfig contains configuration for the health check.

        :param check_interval_sec: CheckIntervalSec is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param healthy_threshold: HealthyThreshold is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param port: 
        :param request_path: RequestPath is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param timeout_sec: TimeoutSec is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param type: Type is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.
        :param unhealthy_threshold: UnhealthyThreshold is a health check parameter. See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__458b54e537fe6beecc1d6be934ee37e0a01c133bc2eaf47c216a85fda2e77928)
            check_type(argname="argument check_interval_sec", value=check_interval_sec, expected_type=type_hints["check_interval_sec"])
            check_type(argname="argument healthy_threshold", value=healthy_threshold, expected_type=type_hints["healthy_threshold"])
            check_type(argname="argument port", value=port, expected_type=type_hints["port"])
            check_type(argname="argument request_path", value=request_path, expected_type=type_hints["request_path"])
            check_type(argname="argument timeout_sec", value=timeout_sec, expected_type=type_hints["timeout_sec"])
            check_type(argname="argument type", value=type, expected_type=type_hints["type"])
            check_type(argname="argument unhealthy_threshold", value=unhealthy_threshold, expected_type=type_hints["unhealthy_threshold"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if check_interval_sec is not None:
            self._values["check_interval_sec"] = check_interval_sec
        if healthy_threshold is not None:
            self._values["healthy_threshold"] = healthy_threshold
        if port is not None:
            self._values["port"] = port
        if request_path is not None:
            self._values["request_path"] = request_path
        if timeout_sec is not None:
            self._values["timeout_sec"] = timeout_sec
        if type is not None:
            self._values["type"] = type
        if unhealthy_threshold is not None:
            self._values["unhealthy_threshold"] = unhealthy_threshold

    @builtins.property
    def check_interval_sec(self) -> typing.Optional[jsii.Number]:
        '''CheckIntervalSec is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck#checkIntervalSec
        '''
        result = self._values.get("check_interval_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def healthy_threshold(self) -> typing.Optional[jsii.Number]:
        '''HealthyThreshold is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck#healthyThreshold
        '''
        result = self._values.get("healthy_threshold")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def port(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigV1Beta1SpecHealthCheck#port
        '''
        result = self._values.get("port")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def request_path(self) -> typing.Optional[builtins.str]:
        '''RequestPath is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck#requestPath
        '''
        result = self._values.get("request_path")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def timeout_sec(self) -> typing.Optional[jsii.Number]:
        '''TimeoutSec is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck#timeoutSec
        '''
        result = self._values.get("timeout_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def type(self) -> typing.Optional[builtins.str]:
        '''Type is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck#type
        '''
        result = self._values.get("type")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def unhealthy_threshold(self) -> typing.Optional[jsii.Number]:
        '''UnhealthyThreshold is a health check parameter.

        See https://cloud.google.com/compute/docs/reference/rest/v1/healthChecks.

        :schema: BackendConfigV1Beta1SpecHealthCheck#unhealthyThreshold
        '''
        result = self._values.get("unhealthy_threshold")
        return typing.cast(typing.Optional[jsii.Number], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecHealthCheck(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecIap",
    jsii_struct_bases=[],
    name_mapping={
        "enabled": "enabled",
        "oauthclient_credentials": "oauthclientCredentials",
    },
)
class BackendConfigV1Beta1SpecIap:
    def __init__(
        self,
        *,
        enabled: builtins.bool,
        oauthclient_credentials: typing.Optional[typing.Union["BackendConfigV1Beta1SpecIapOauthclientCredentials", typing.Dict[builtins.str, typing.Any]]] = None,
    ) -> None:
        '''IAPConfig contains configuration for IAP-enabled backends.

        :param enabled: 
        :param oauthclient_credentials: OAuthClientCredentials contains credentials for a single IAP-enabled backend.

        :schema: BackendConfigV1Beta1SpecIap
        '''
        if isinstance(oauthclient_credentials, dict):
            oauthclient_credentials = BackendConfigV1Beta1SpecIapOauthclientCredentials(**oauthclient_credentials)
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__648ecbb43d6288293062736e62bd90212e194b4583cd093010af134205fc20ae)
            check_type(argname="argument enabled", value=enabled, expected_type=type_hints["enabled"])
            check_type(argname="argument oauthclient_credentials", value=oauthclient_credentials, expected_type=type_hints["oauthclient_credentials"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "enabled": enabled,
        }
        if oauthclient_credentials is not None:
            self._values["oauthclient_credentials"] = oauthclient_credentials

    @builtins.property
    def enabled(self) -> builtins.bool:
        '''
        :schema: BackendConfigV1Beta1SpecIap#enabled
        '''
        result = self._values.get("enabled")
        assert result is not None, "Required property 'enabled' is missing"
        return typing.cast(builtins.bool, result)

    @builtins.property
    def oauthclient_credentials(
        self,
    ) -> typing.Optional["BackendConfigV1Beta1SpecIapOauthclientCredentials"]:
        '''OAuthClientCredentials contains credentials for a single IAP-enabled backend.

        :schema: BackendConfigV1Beta1SpecIap#oauthclientCredentials
        '''
        result = self._values.get("oauthclient_credentials")
        return typing.cast(typing.Optional["BackendConfigV1Beta1SpecIapOauthclientCredentials"], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecIap(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecIapOauthclientCredentials",
    jsii_struct_bases=[],
    name_mapping={
        "secret_name": "secretName",
        "client_id": "clientId",
        "client_secret": "clientSecret",
    },
)
class BackendConfigV1Beta1SpecIapOauthclientCredentials:
    def __init__(
        self,
        *,
        secret_name: builtins.str,
        client_id: typing.Optional[builtins.str] = None,
        client_secret: typing.Optional[builtins.str] = None,
    ) -> None:
        '''OAuthClientCredentials contains credentials for a single IAP-enabled backend.

        :param secret_name: The name of a k8s secret which stores the OAuth client id & secret.
        :param client_id: Direct reference to OAuth client id.
        :param client_secret: Direct reference to OAuth client secret.

        :schema: BackendConfigV1Beta1SpecIapOauthclientCredentials
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__0dc29a4b06fd6606e4aea860922706594c9ec38e4b0615449a84ea6f91315453)
            check_type(argname="argument secret_name", value=secret_name, expected_type=type_hints["secret_name"])
            check_type(argname="argument client_id", value=client_id, expected_type=type_hints["client_id"])
            check_type(argname="argument client_secret", value=client_secret, expected_type=type_hints["client_secret"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "secret_name": secret_name,
        }
        if client_id is not None:
            self._values["client_id"] = client_id
        if client_secret is not None:
            self._values["client_secret"] = client_secret

    @builtins.property
    def secret_name(self) -> builtins.str:
        '''The name of a k8s secret which stores the OAuth client id & secret.

        :schema: BackendConfigV1Beta1SpecIapOauthclientCredentials#secretName
        '''
        result = self._values.get("secret_name")
        assert result is not None, "Required property 'secret_name' is missing"
        return typing.cast(builtins.str, result)

    @builtins.property
    def client_id(self) -> typing.Optional[builtins.str]:
        '''Direct reference to OAuth client id.

        :schema: BackendConfigV1Beta1SpecIapOauthclientCredentials#clientID
        '''
        result = self._values.get("client_id")
        return typing.cast(typing.Optional[builtins.str], result)

    @builtins.property
    def client_secret(self) -> typing.Optional[builtins.str]:
        '''Direct reference to OAuth client secret.

        :schema: BackendConfigV1Beta1SpecIapOauthclientCredentials#clientSecret
        '''
        result = self._values.get("client_secret")
        return typing.cast(typing.Optional[builtins.str], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecIapOauthclientCredentials(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecSecurityPolicy",
    jsii_struct_bases=[],
    name_mapping={"name": "name"},
)
class BackendConfigV1Beta1SpecSecurityPolicy:
    def __init__(self, *, name: builtins.str) -> None:
        '''SecurityPolicyConfig contains configuration for CloudArmor-enabled backends.

        If not specified, the controller will not reconcile the security policy configuration. In other words, users can make changes in GCE without the controller overwriting them.

        :param name: Name of the security policy that should be associated. If set to empty, the existing security policy on the backend will be removed.

        :schema: BackendConfigV1Beta1SpecSecurityPolicy
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__cdce0ae495867c2f18a2d5ba1ae20e252cbd90ac9ef9d6349e5bff94e9927b27)
            check_type(argname="argument name", value=name, expected_type=type_hints["name"])
        self._values: typing.Dict[builtins.str, typing.Any] = {
            "name": name,
        }

    @builtins.property
    def name(self) -> builtins.str:
        '''Name of the security policy that should be associated.

        If set to empty, the existing security policy on the backend will be removed.

        :schema: BackendConfigV1Beta1SpecSecurityPolicy#name
        '''
        result = self._values.get("name")
        assert result is not None, "Required property 'name' is missing"
        return typing.cast(builtins.str, result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecSecurityPolicy(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


@jsii.data_type(
    jsii_type="comgooglecloud.BackendConfigV1Beta1SpecSessionAffinity",
    jsii_struct_bases=[],
    name_mapping={
        "affinity_cookie_ttl_sec": "affinityCookieTtlSec",
        "affinity_type": "affinityType",
    },
)
class BackendConfigV1Beta1SpecSessionAffinity:
    def __init__(
        self,
        *,
        affinity_cookie_ttl_sec: typing.Optional[jsii.Number] = None,
        affinity_type: typing.Optional[builtins.str] = None,
    ) -> None:
        '''SessionAffinityConfig contains configuration for stickiness parameters.

        :param affinity_cookie_ttl_sec: 
        :param affinity_type: 

        :schema: BackendConfigV1Beta1SpecSessionAffinity
        '''
        if __debug__:
            type_hints = typing.get_type_hints(_typecheckingstub__4538d5a95a2d339f0245126e5974162172c3eac10c8949e1b5d01fc77dc1a8ea)
            check_type(argname="argument affinity_cookie_ttl_sec", value=affinity_cookie_ttl_sec, expected_type=type_hints["affinity_cookie_ttl_sec"])
            check_type(argname="argument affinity_type", value=affinity_type, expected_type=type_hints["affinity_type"])
        self._values: typing.Dict[builtins.str, typing.Any] = {}
        if affinity_cookie_ttl_sec is not None:
            self._values["affinity_cookie_ttl_sec"] = affinity_cookie_ttl_sec
        if affinity_type is not None:
            self._values["affinity_type"] = affinity_type

    @builtins.property
    def affinity_cookie_ttl_sec(self) -> typing.Optional[jsii.Number]:
        '''
        :schema: BackendConfigV1Beta1SpecSessionAffinity#affinityCookieTtlSec
        '''
        result = self._values.get("affinity_cookie_ttl_sec")
        return typing.cast(typing.Optional[jsii.Number], result)

    @builtins.property
    def affinity_type(self) -> typing.Optional[builtins.str]:
        '''
        :schema: BackendConfigV1Beta1SpecSessionAffinity#affinityType
        '''
        result = self._values.get("affinity_type")
        return typing.cast(typing.Optional[builtins.str], result)

    def __eq__(self, rhs: typing.Any) -> builtins.bool:
        return isinstance(rhs, self.__class__) and rhs._values == self._values

    def __ne__(self, rhs: typing.Any) -> builtins.bool:
        return not (rhs == self)

    def __repr__(self) -> str:
        return "BackendConfigV1Beta1SpecSessionAffinity(%s)" % ", ".join(
            k + "=" + repr(v) for k, v in self._values.items()
        )


__all__ = [
    "BackendConfig",
    "BackendConfigProps",
    "BackendConfigSpec",
    "BackendConfigSpecCdn",
    "BackendConfigSpecCdnBypassCacheOnRequestHeaders",
    "BackendConfigSpecCdnCachePolicy",
    "BackendConfigSpecCdnNegativeCachingPolicy",
    "BackendConfigSpecCdnSignedUrlKeys",
    "BackendConfigSpecConnectionDraining",
    "BackendConfigSpecCustomRequestHeaders",
    "BackendConfigSpecCustomResponseHeaders",
    "BackendConfigSpecHealthCheck",
    "BackendConfigSpecIap",
    "BackendConfigSpecIapOauthclientCredentials",
    "BackendConfigSpecLogging",
    "BackendConfigSpecSecurityPolicy",
    "BackendConfigSpecSessionAffinity",
    "BackendConfigV1Beta1",
    "BackendConfigV1Beta1Props",
    "BackendConfigV1Beta1Spec",
    "BackendConfigV1Beta1SpecCdn",
    "BackendConfigV1Beta1SpecCdnCachePolicy",
    "BackendConfigV1Beta1SpecConnectionDraining",
    "BackendConfigV1Beta1SpecCustomRequestHeaders",
    "BackendConfigV1Beta1SpecHealthCheck",
    "BackendConfigV1Beta1SpecIap",
    "BackendConfigV1Beta1SpecIapOauthclientCredentials",
    "BackendConfigV1Beta1SpecSecurityPolicy",
    "BackendConfigV1Beta1SpecSessionAffinity",
]

publication.publish()

def _typecheckingstub__478e053b34646f2f22c316b46d46fa578352a17dc0965e0733c3edbb3af21e40(
    scope: _constructs_77d1e7e8.Construct,
    id: builtins.str,
    *,
    metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
    spec: typing.Optional[typing.Union[BackendConfigSpec, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__e8e7a740219c8083806cfb80142ff0861537e30e169b215266207dac2fc18296(
    *,
    metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
    spec: typing.Optional[typing.Union[BackendConfigSpec, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__ed3aaa52e5e9ad8c8ce8edc12e3f41b1303bc88c9e4d6d58ade780d4014f2019(
    *,
    cdn: typing.Optional[typing.Union[BackendConfigSpecCdn, typing.Dict[builtins.str, typing.Any]]] = None,
    connection_draining: typing.Optional[typing.Union[BackendConfigSpecConnectionDraining, typing.Dict[builtins.str, typing.Any]]] = None,
    custom_request_headers: typing.Optional[typing.Union[BackendConfigSpecCustomRequestHeaders, typing.Dict[builtins.str, typing.Any]]] = None,
    custom_response_headers: typing.Optional[typing.Union[BackendConfigSpecCustomResponseHeaders, typing.Dict[builtins.str, typing.Any]]] = None,
    health_check: typing.Optional[typing.Union[BackendConfigSpecHealthCheck, typing.Dict[builtins.str, typing.Any]]] = None,
    iap: typing.Optional[typing.Union[BackendConfigSpecIap, typing.Dict[builtins.str, typing.Any]]] = None,
    logging: typing.Optional[typing.Union[BackendConfigSpecLogging, typing.Dict[builtins.str, typing.Any]]] = None,
    security_policy: typing.Optional[typing.Union[BackendConfigSpecSecurityPolicy, typing.Dict[builtins.str, typing.Any]]] = None,
    session_affinity: typing.Optional[typing.Union[BackendConfigSpecSessionAffinity, typing.Dict[builtins.str, typing.Any]]] = None,
    timeout_sec: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__622a4ce6676e87a1185b566cba87d43a91644ca1bc40dd893f4fac498eb82531(
    *,
    enabled: builtins.bool,
    bypass_cache_on_request_headers: typing.Optional[typing.Sequence[typing.Union[BackendConfigSpecCdnBypassCacheOnRequestHeaders, typing.Dict[builtins.str, typing.Any]]]] = None,
    cache_mode: typing.Optional[builtins.str] = None,
    cache_policy: typing.Optional[typing.Union[BackendConfigSpecCdnCachePolicy, typing.Dict[builtins.str, typing.Any]]] = None,
    client_ttl: typing.Optional[jsii.Number] = None,
    default_ttl: typing.Optional[jsii.Number] = None,
    max_ttl: typing.Optional[jsii.Number] = None,
    negative_caching: typing.Optional[builtins.bool] = None,
    negative_caching_policy: typing.Optional[typing.Sequence[typing.Union[BackendConfigSpecCdnNegativeCachingPolicy, typing.Dict[builtins.str, typing.Any]]]] = None,
    request_coalescing: typing.Optional[builtins.bool] = None,
    serve_while_stale: typing.Optional[jsii.Number] = None,
    signed_url_cache_max_age_sec: typing.Optional[jsii.Number] = None,
    signed_url_keys: typing.Optional[typing.Sequence[typing.Union[BackendConfigSpecCdnSignedUrlKeys, typing.Dict[builtins.str, typing.Any]]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__29143d667e3ffdd72b7660770c144cc283425c836b9727b5dce6d9ad54507331(
    *,
    header_name: typing.Optional[builtins.str] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__58baffb66190c78e8d4c5f00de26d9625e7d2a0a6d8e74e5662211067b562a23(
    *,
    include_host: typing.Optional[builtins.bool] = None,
    include_protocol: typing.Optional[builtins.bool] = None,
    include_query_string: typing.Optional[builtins.bool] = None,
    query_string_blacklist: typing.Optional[typing.Sequence[builtins.str]] = None,
    query_string_whitelist: typing.Optional[typing.Sequence[builtins.str]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__7df89c988b317eecced15460557d2535638cb9d58470bd2e37718341ca8102b6(
    *,
    code: typing.Optional[jsii.Number] = None,
    ttl: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__c2bfc9b5d277ae2443b64ec9704ba599608ab4b41c297adb864d61ad89882fad(
    *,
    key_name: typing.Optional[builtins.str] = None,
    key_value: typing.Optional[builtins.str] = None,
    secret_name: typing.Optional[builtins.str] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__a8a4d781a8209b710ae791e501ffc537c4e1abe71291f6bfe94b802fde855fc0(
    *,
    draining_timeout_sec: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__8dc5d6b2f96616a24acd3ccf5a6df73b067f9250a84a34ae8d3eb2d6996a106d(
    *,
    headers: typing.Optional[typing.Sequence[builtins.str]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__3d55a2d2ade28e64a421578946506b571eb485866dc843b5f94b163bf1b36867(
    *,
    headers: typing.Optional[typing.Sequence[builtins.str]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__79c151f1f31cafead3943aeaed93e335f4e87ceb8da1e792180414251cfe761a(
    *,
    check_interval_sec: typing.Optional[jsii.Number] = None,
    healthy_threshold: typing.Optional[jsii.Number] = None,
    port: typing.Optional[jsii.Number] = None,
    request_path: typing.Optional[builtins.str] = None,
    timeout_sec: typing.Optional[jsii.Number] = None,
    type: typing.Optional[builtins.str] = None,
    unhealthy_threshold: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__13c7a07f21084397e395ce02eb15e5467ad5a76b4b555f0d12b8d64cd9eb3a84(
    *,
    enabled: builtins.bool,
    oauthclient_credentials: typing.Optional[typing.Union[BackendConfigSpecIapOauthclientCredentials, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__9ce01ce36f37e0da11c74ffa5ee924df4b65d1e3adbf2ea4b16241b79f60c21e(
    *,
    secret_name: builtins.str,
    client_id: typing.Optional[builtins.str] = None,
    client_secret: typing.Optional[builtins.str] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__70de348aa7cf778b7313e13ebebbfdd0982a935c020de62a80311862fc6623a9(
    *,
    enable: typing.Optional[builtins.bool] = None,
    sample_rate: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__98ab3b68c0a293fb2cbc562276ebfc79835049626e8b0cd436f56437f7ef2aad(
    *,
    name: builtins.str,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__12b36eeaa8e78922a88283ed58641b01eb9642275277cb5d79c87068d3e196b8(
    *,
    affinity_cookie_ttl_sec: typing.Optional[jsii.Number] = None,
    affinity_type: typing.Optional[builtins.str] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__344b0717d420cfea1a675b9b85d42abd06963df4f4e2b6703b26b6c888941945(
    scope: _constructs_77d1e7e8.Construct,
    id: builtins.str,
    *,
    metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
    spec: typing.Optional[typing.Union[BackendConfigV1Beta1Spec, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__f97dd20787fe9d41b4c4414f7cf16853f3f154133fed2b3101de407e38a242f6(
    *,
    metadata: typing.Optional[typing.Union[_cdk8s_d3d9af27.ApiObjectMetadata, typing.Dict[builtins.str, typing.Any]]] = None,
    spec: typing.Optional[typing.Union[BackendConfigV1Beta1Spec, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__ce76bcc047783b5343fff4e7a4014ad78591646cde2f19b57180e83c0addac84(
    *,
    cdn: typing.Optional[typing.Union[BackendConfigV1Beta1SpecCdn, typing.Dict[builtins.str, typing.Any]]] = None,
    connection_draining: typing.Optional[typing.Union[BackendConfigV1Beta1SpecConnectionDraining, typing.Dict[builtins.str, typing.Any]]] = None,
    custom_request_headers: typing.Optional[typing.Union[BackendConfigV1Beta1SpecCustomRequestHeaders, typing.Dict[builtins.str, typing.Any]]] = None,
    health_check: typing.Optional[typing.Union[BackendConfigV1Beta1SpecHealthCheck, typing.Dict[builtins.str, typing.Any]]] = None,
    iap: typing.Optional[typing.Union[BackendConfigV1Beta1SpecIap, typing.Dict[builtins.str, typing.Any]]] = None,
    security_policy: typing.Optional[typing.Union[BackendConfigV1Beta1SpecSecurityPolicy, typing.Dict[builtins.str, typing.Any]]] = None,
    session_affinity: typing.Optional[typing.Union[BackendConfigV1Beta1SpecSessionAffinity, typing.Dict[builtins.str, typing.Any]]] = None,
    timeout_sec: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__7356f4b96f97aa57f7f1d1e7d1ee04d987bafe3588ef34aee1d27a535380fb82(
    *,
    enabled: builtins.bool,
    cache_policy: typing.Optional[typing.Union[BackendConfigV1Beta1SpecCdnCachePolicy, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__f3de908e6a83c0f93d3ad698e79d4a79b4f86b58a6f774bedbd980fbdf53e06d(
    *,
    include_host: typing.Optional[builtins.bool] = None,
    include_protocol: typing.Optional[builtins.bool] = None,
    include_query_string: typing.Optional[builtins.bool] = None,
    query_string_blacklist: typing.Optional[typing.Sequence[builtins.str]] = None,
    query_string_whitelist: typing.Optional[typing.Sequence[builtins.str]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__c6f2fcacdc49b8b8096a586cf8c3e9c44397b88e90594778bad466213d4d3f38(
    *,
    draining_timeout_sec: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__778f47b4768dec1ed1170f49b440363a1f9082599e4b897535aa179f840e7c2e(
    *,
    headers: typing.Optional[typing.Sequence[builtins.str]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__458b54e537fe6beecc1d6be934ee37e0a01c133bc2eaf47c216a85fda2e77928(
    *,
    check_interval_sec: typing.Optional[jsii.Number] = None,
    healthy_threshold: typing.Optional[jsii.Number] = None,
    port: typing.Optional[jsii.Number] = None,
    request_path: typing.Optional[builtins.str] = None,
    timeout_sec: typing.Optional[jsii.Number] = None,
    type: typing.Optional[builtins.str] = None,
    unhealthy_threshold: typing.Optional[jsii.Number] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__648ecbb43d6288293062736e62bd90212e194b4583cd093010af134205fc20ae(
    *,
    enabled: builtins.bool,
    oauthclient_credentials: typing.Optional[typing.Union[BackendConfigV1Beta1SpecIapOauthclientCredentials, typing.Dict[builtins.str, typing.Any]]] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__0dc29a4b06fd6606e4aea860922706594c9ec38e4b0615449a84ea6f91315453(
    *,
    secret_name: builtins.str,
    client_id: typing.Optional[builtins.str] = None,
    client_secret: typing.Optional[builtins.str] = None,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__cdce0ae495867c2f18a2d5ba1ae20e252cbd90ac9ef9d6349e5bff94e9927b27(
    *,
    name: builtins.str,
) -> None:
    """Type checking stubs"""
    pass

def _typecheckingstub__4538d5a95a2d339f0245126e5974162172c3eac10c8949e1b5d01fc77dc1a8ea(
    *,
    affinity_cookie_ttl_sec: typing.Optional[jsii.Number] = None,
    affinity_type: typing.Optional[builtins.str] = None,
) -> None:
    """Type checking stubs"""
    pass
