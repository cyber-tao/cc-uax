pub mod ue5 {
    pub const INITIAL_VERSION: i32 = 1000;
    pub const NAMES_REFERENCED_FROM_EXPORT_DATA: i32 = 1001;
    pub const PAYLOAD_TOC: i32 = 1002;
    pub const OPTIONAL_RESOURCES: i32 = 1003;
    pub const LARGE_WORLD_COORDINATES: i32 = 1004;
    pub const REMOVE_OBJECT_EXPORT_PACKAGE_GUID: i32 = 1005;
    pub const TRACK_OBJECT_EXPORT_IS_INHERITED: i32 = 1006;
    pub const FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES: i32 = 1007;
    pub const ADD_SOFTOBJECTPATH_LIST: i32 = 1008;
    pub const DATA_RESOURCES: i32 = 1009;
    pub const SCRIPT_SERIALIZATION_OFFSET: i32 = 1010;
    pub const PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION: i32 = 1011;
    pub const PROPERTY_TAG_COMPLETE_TYPE_NAME: i32 = 1012;
    pub const ASSETREGISTRY_PACKAGEBUILDDEPENDENCIES: i32 = 1013;
    pub const METADATA_SERIALIZATION_OFFSET: i32 = 1014;
    pub const VERSE_CELLS: i32 = 1015;
    pub const PACKAGE_SAVED_HASH: i32 = 1016;
    pub const OS_SUB_OBJECT_SHADOW_SERIALIZATION: i32 = 1017;
    pub const IMPORT_TYPE_HIERARCHIES: i32 = 1018;
}

pub mod ue4 {
    pub const WORLD_LEVEL_INFO: i32 = 224;
    pub const ADDED_CHUNKID_TO_ASSETDATA_AND_UPACKAGE: i32 = 278;
    pub const ENGINE_VERSION_OBJECT: i32 = 336;
    pub const LOAD_FOR_EDITOR_GAME: i32 = 365;
    pub const ADD_STRING_ASSET_REFERENCES_MAP: i32 = 384;
    pub const CHANGED_CHUNKID_TO_BE_AN_ARRAY_OF_CHUNKIDS: i32 = 392;
    pub const COOKED_ASSETS_IN_EDITOR_SUPPORT: i32 = 415;
    pub const STRUCT_GUID_IN_PROPERTY_TAG: i32 = 441;
    pub const PACKAGE_SUMMARY_HAS_COMPATIBLE_ENGINE_VERSION: i32 = 444;
    pub const SERIALIZE_TEXT_IN_PACKAGES: i32 = 459;
    pub const INNER_ARRAY_TAG_INFO: i32 = 500;
    pub const PROPERTY_GUID_IN_PROPERTY_TAG: i32 = 503;
    pub const NAME_HASHES_SERIALIZED: i32 = 504;
    pub const PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS: i32 = 507;
    pub const TEMPLATEINDEX_IN_COOKED_EXPORTS: i32 = 508;
    pub const PROPERTY_TAG_SET_MAP_SUPPORT: i32 = 509;
    pub const ADDED_SEARCHABLE_NAMES: i32 = 510;
    pub const SERIALSIZE_64BIT_EXPORTMAP: i32 = 511;
    pub const ADDED_SOFT_OBJECT_PATH: i32 = 514;
    pub const ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID: i32 = 516;
    pub const ADDED_PACKAGE_OWNER: i32 = 518;
    pub const NON_OUTER_PACKAGE_IMPORT: i32 = 520;

    pub const HIGHEST: i32 = 522;
}

pub mod custom {
    use crate::reader::Guid;

    pub const FRAMEWORK_OBJECT_VERSION: Guid =
        Guid([0xCFFC_743F, 0x43B0_4480, 0x9391_14DF, 0x171D_2073]);
    pub const BLUEPRINTS_OBJECT_VERSION: Guid =
        Guid([0xB0D8_32E4, 0x1F89_4F0D, 0xACCF_7EB7, 0x36FD_4AA2]);
    pub const RELEASE_OBJECT_VERSION: Guid =
        Guid([0x9C54_D522, 0xA826_4FBE, 0x9421_0746, 0x61B4_82D0]);
    pub const UE5_MAIN_STREAM_OBJECT_VERSION: Guid =
        Guid([0x697D_D581, 0xE64F_41AB, 0xAA4A_51EC, 0xBEB7_B628]);
    pub const UE5_RELEASE_STREAM_OBJECT_VERSION: Guid =
        Guid([0xD89B_5E42, 0x24BD_4D46, 0x8412_ACA8, 0xDF64_1779]);
    pub const NIAGARA_OBJECT_VERSION: Guid =
        Guid([0xFCF5_7AFA, 0x5076_4283, 0xB9A9_E658, 0xFFA0_2D32]);
    pub const FORTNITE_MAIN_OBJECT_VERSION: Guid =
        Guid([0x601D_1886, 0xAC64_4F84, 0xAA16_D3DE, 0x0DEA_C7D6]);

    pub const EDGRAPH_PIN_SOURCE_INDEX: i32 = 50;
    /// FFortniteMainBranchObjectVersion::SerializeFloatChannelShowCurve — from this
    /// version on, MovieScene float/double channels serialize a trailing bShowCurve.
    pub const SERIALIZE_FLOAT_CHANNEL_SHOW_CURVE: i32 = 53;
    pub const NIAGARA_ADD_GENERATED_FUNCTIONS_TO_GPU_PARAM_INFO: i32 = 55;
    /// FNiagaraCustomVersion::VariablesUseTypeDefRegistry — from this version on,
    /// FNiagaraVariableBase serializes Name + a tagged-property FNiagaraTypeDefinition.
    pub const NIAGARA_VARIABLES_USE_TYPE_DEF_REGISTRY: i32 = 64;
    pub const NIAGARA_ADD_VARIADIC_PARAMETERS_TO_GPU_FUNCTION_INFO: i32 = 77;
    pub const NIAGARA_SERIALIZE_USAGE_BITMASK_TO_GPU_FUNCTION_INFO: i32 = 91;
    pub const PIN_TYPE_INCLUDES_UOBJECT_WRAPPER_FLAG: i32 = 32;
    pub const SERIALIZE_FLOAT_PIN_SINGLE_PRECISION: i32 = 36;
}

pub const PACKAGE_FILE_TAG: u32 = 0x9E2A_83C1;

pub const PACKAGE_FILE_TAG_SWAPPED: u32 = 0xC183_2A9E;
