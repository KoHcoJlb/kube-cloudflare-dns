use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use kube::Resource;

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct ResourceKey {
    pub kind: String,
    pub namespace: String,
    pub name: String,
}

impl ResourceKey {
    pub fn from<T: Resource>(res: &T) -> Self
        where <T as Resource>::DynamicType: Default {
        Self {
            kind: T::kind(&Default::default()).into(),
            name: res.meta().name.clone().unwrap(),
            namespace: res.meta().namespace.clone().unwrap(),
        }
    }
}

#[derive(Debug)]
pub enum WatchedResource {
    Ingress(Ingress),
    Service(Service),
}

impl From<Service> for WatchedResource {
    fn from(service: Service) -> Self {
        Self::Service(service)
    }
}

impl From<Ingress> for WatchedResource {
    fn from(ingress: Ingress) -> Self {
        Self::Ingress(ingress)
    }
}
