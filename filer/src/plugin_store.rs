use super::fs::*;

use abi_stable::{
    abi_stability::check_layout_compatibility, std_types::UTypeId, type_layout::TypeLayout,
    StableAbi,
};
use cglue::prelude::v1::*;
use cglue::trait_group::c_void;

use dashmap::mapref::entry;
use dashmap::DashMap;
use sharded_slab::{Entry, Slab};

pub type MappingFunction<T, O> = extern "C" fn(&T, &CArc<c_void>) -> COption<O>;

#[derive(StableAbi)]
// TODO: Why does the func type not work??
#[sabi(unsafe_opaque_fields)]
#[repr(C)]
pub enum Mapping<T: StableAbi> {
    Branch(MappingFunction<T, BranchArcBox<'static>>, CArc<c_void>),
    Leaf(MappingFunction<T, LeafArcBox<'static>>, CArc<c_void>),
}

unsafe impl<T: StableAbi> Opaquable for Mapping<T> {
    type OpaqueTarget = Mapping<c_void>;
}

impl<T: StableAbi> Clone for Mapping<T> {
    fn clone(&self) -> Self {
        match self {
            Mapping::Branch(a, b) => Mapping::Branch(*a, b.clone()),
            Mapping::Leaf(a, b) => Mapping::Leaf(*a, b.clone()),
        }
    }
}

type OpaqueMapping = Mapping<c_void>;

#[derive(Default)]
pub struct PluginStore {
    // plugins: Vec<Box<dyn Plugin>>
    /// A never shrinking list of lists of mapping functions meant for specific types of plugins.
    ///
    /// Calling the mapping functions manually is inherently unsafe, because the types are meant to
    /// be opaque, and unchecked downcasting is being performed.
    entry_list: Slab<DashMap<String, OpaqueMapping>>,
    /// Type layouts identified by slab index.
    layouts: DashMap<usize, &'static TypeLayout>,
    /// Map a specific type to opaque entries in the list.
    type_map: DashMap<UTypeId, usize>,
}

#[derive(StableAbi)]
#[repr(C)]
pub struct CPluginStore {
    store: CBox<'static, c_void>,
    lookup_entry: for<'a> unsafe extern "C" fn(
        &'a c_void,
        UTypeId,
        &'static TypeLayout,
        CSliceRef<'a, u8>,
    ) -> COption<OpaqueMapping>,
    entry_list: for<'a> unsafe extern "C" fn(
        &'a c_void,
        UTypeId,
        &'static TypeLayout,
        &mut OpaqueCallback<*const c_void>,
    ),
    register_mapping: for<'a> unsafe extern "C" fn(
        &'a c_void,
        UTypeId,
        &'static TypeLayout,
        CSliceRef<'a, u8>,
        OpaqueMapping,
    ) -> bool,
}

impl Default for CPluginStore {
    fn default() -> Self {
        PluginStore::default().into()
    }
}

impl From<PluginStore> for CPluginStore {
    fn from(store: PluginStore) -> Self {
        unsafe extern "C" fn lookup_entry(
            store: &c_void,
            id: UTypeId,
            layout: &'static TypeLayout,
            name: CSliceRef<u8>,
        ) -> COption<OpaqueMapping> {
            let store = store as *const _ as *const PluginStore;
            let entries = (*store).entries_raw(id, layout);
            entries.get(name.into_str()).map(|e| (*e).clone()).into()
        }

        unsafe extern "C" fn entry_list(
            store: &c_void,
            id: UTypeId,
            layout: &'static TypeLayout,
            out: &mut OpaqueCallback<*const c_void>,
        ) {
            let store: &PluginStore = &*(store as *const _ as *const PluginStore);
            let entries = (*store).entries_raw(id, layout);
            entries
                .iter()
                .take_while(|e| {
                    out.call(
                        &CTup2(CSliceRef::from(e.key().as_str()), (*e.value()).clone()) as *const _
                            as *const c_void,
                    )
                })
                .for_each(|_| {});
        }

        unsafe extern "C" fn register_mapping(
            store: &c_void,
            id: UTypeId,
            layout: &'static TypeLayout,
            name: CSliceRef<u8>,
            mapping: OpaqueMapping,
        ) -> bool {
            let store = store as *const _ as *const PluginStore;
            (*store).register_mapping_raw(id, layout, name.into_str(), mapping)
        }

        Self {
            store: CBox::from(store).into_opaque(),
            lookup_entry,
            entry_list,
            register_mapping,
        }
    }
}

impl CPluginStore {
    pub fn register_mapping<T: StableAbi>(&self, name: &str, mapping: Mapping<T>) -> bool {
        unsafe {
            (self.register_mapping)(
                &*self.store,
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                name.into(),
                mapping.into_opaque(),
            )
        }
    }

    pub fn lookup_entry<T: StableAbi>(&self, name: &str) -> Option<Mapping<T>> {
        let mapping: Option<OpaqueMapping> = unsafe {
            (self.lookup_entry)(
                &*self.store,
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                name.into(),
            )
        }
        .into();
        unsafe { core::mem::transmute(mapping) }
    }

    pub fn entry_list<'a, T: StableAbi>(
        &'a self,
        mut callback: OpaqueCallback<(&'a str, &'a Mapping<T>)>,
    ) {
        let cb = &mut move |data: *const c_void| {
            let CTup2(a, b): &CTup2<CSliceRef<'a, u8>, OpaqueMapping> =
                unsafe { &*(data as *const c_void as *const _) };
            callback.call((unsafe { a.into_str() }, unsafe {
                &*(b as *const OpaqueMapping as *const Mapping<T>)
            }))
        };

        unsafe {
            (self.entry_list)(
                &*self.store,
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                &mut cb.into(),
            )
        };
    }
}

impl PluginStore {
    pub unsafe fn entries_raw(
        &self,
        id: UTypeId,
        layout: &'static TypeLayout,
    ) -> Entry<DashMap<String, OpaqueMapping>> {
        let idx = *self.type_map.entry(id).or_insert_with(|| {
            let (idx, inserted) = self
                .layouts
                .iter()
                .find(|p| check_layout_compatibility(layout, p.value()).is_ok())
                .map(|p| (*p.key(), true))
                .or_else(|| {
                    self.entry_list
                        .insert(Default::default())
                        .map(|i| (i, false))
                })
                .expect("Slab is full!");

            if !inserted {
                self.layouts.insert(idx, layout);
            }

            idx
        });

        self.entry_list.get(idx).unwrap()
    }

    pub fn entries<T: StableAbi>(&self) -> Entry<DashMap<String, Mapping<T>>> {
        let id = T::ABI_CONSTS.type_id.get();
        unsafe { std::mem::transmute(self.entries_raw(id, T::LAYOUT)) }
    }

    pub unsafe fn register_mapping_raw(
        &self,
        id: UTypeId,
        layout: &'static TypeLayout,
        name: &str,
        mapping: OpaqueMapping,
    ) -> bool {
        let entries = self.entries_raw(id, layout);

        let entry = entries.entry(name.to_string());

        if matches!(entry, entry::Entry::Vacant(_)) {
            entry.or_insert(mapping);
            true
        } else {
            false
        }
    }

    pub fn register_mapping<T: StableAbi>(&self, name: &str, mapping: Mapping<T>) -> bool {
        unsafe {
            self.register_mapping_raw(
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                name,
                mapping.into_opaque(),
            )
        }
    }
}
