
#![allow(non_upper_case_globals)]

use std::io::{Read, Result};
use std::marker::PhantomData;
use byteorder::{ReadBytesExt, LittleEndian};

use loader::stream::{TableId, IndexSize, RowCounts, FieldSizes};

pub trait ReadIndexSizeExt {
  fn read_index<T>(&mut self, size: IndexSize) -> Result<Index<T>>;
}

impl<T: Read> ReadIndexSizeExt for T {
  fn read_index<I>(&mut self, size: IndexSize) -> Result<Index<I>> {
    match size {
      IndexSize::Word => self.read_u16::<LittleEndian>().map(|x| x as u32),
      IndexSize::Dword => self.read_u32::<LittleEndian>()
    }.map(|x| Index::new(x))
  }
}

#[derive(Debug, Copy, Clone)]
pub struct Index<T>(pub u32, PhantomData<T>);

pub trait TableEntryReader {
  fn read_entry<R: Read>(reader: &mut R, sizes: &FieldSizes) -> Result<Self> where Self : Sized;
}

impl<T> Index<T> {
  fn new(index: u32) -> Index<T> {
    Index(index, PhantomData::<T>)
  }

  fn to_u32(self) -> u32 {
    self.0
  }
}

#[derive(Debug)]
pub struct ModuleEntry {
  generation: u16,
  name: Index<StringHeap>,
  mv_id: Index<GuidHeap>,
  enc_id: Index<GuidHeap>,
  enc_base_id: Index<GuidHeap>
}

#[derive(Debug)]
pub struct ModuleRefEntry { }

#[derive(Debug)]
pub struct AssemblyRefEntry { }

#[derive(Debug)]
pub struct FieldEntry { }

#[derive(Debug)]
pub struct ParamEntry { }

#[derive(Debug)]
pub struct PropertyEntry { }

pub struct AssemblyEntry { }

pub struct InterfaceImplEntry { }

pub struct MethodDefEntry { }

pub struct MethodRefEntry { }

pub struct StandAloneSigEntry { }

pub struct EventEntry { }

pub struct PermissionEntry { }

pub struct MemberRefEntry { }

pub struct FileEntry { }

pub struct ExportedTypeEntry { }

pub struct ManifestResourceEntry { }

#[derive(Debug)]
pub struct TypeRefEntry {
  resolution_scope: ResolutionScope,
  name: Index<StringHeap>,
  namespace: Index<StringHeap>
}

#[derive(Debug)]
pub struct TypeDefEntry {
  flags: TypeAttributes,
  name: Index<StringHeap>,
  namespace: Index<StringHeap>,
  extends: TypeDefOrRef
}

#[derive(Debug)]
pub struct TypeSpecEntry {
  signature: Index<BlobHeap>
}

impl TableEntryReader for ModuleEntry {
  fn read_entry<R: Read>(reader: &mut R, sizes: &FieldSizes) -> Result<ModuleEntry> {
    let generation = reader.read_u16::<LittleEndian>()?;
    let name = reader.read_index(sizes.heap_sizes.string_index)?;
    let mv_id = reader.read_index(sizes.heap_sizes.guid_index)?;
    let enc_id = reader.read_index(sizes.heap_sizes.guid_index)?;
    let enc_base_id = reader.read_index(sizes.heap_sizes.guid_index)?;

    Ok(ModuleEntry {generation, name, mv_id, enc_id, enc_base_id} )
  }
}

impl TableEntryReader for TypeRefEntry {
  fn read_entry<R: Read>(reader: &mut R, sizes: &FieldSizes) -> Result<TypeRefEntry> {
    let resolution_scope = ResolutionScope::read_from(reader, &sizes.row_counts)?;
    let name = reader.read_index(sizes.heap_sizes.string_index)?;
    let namespace = reader.read_index(sizes.heap_sizes.string_index)?;

    Ok(TypeRefEntry { resolution_scope, name, namespace })
  }
}

impl TableEntryReader for TypeDefEntry {
  fn read_entry<R: Read>(reader: &mut R, sizes: &FieldSizes) -> Result<TypeDefEntry> {
    let flags_encoded = reader.read_u32::<LittleEndian>()?;
    let flags = TypeAttributes::from_bits(flags_encoded).unwrap();
    let name = reader.read_index(sizes.heap_sizes.string_index)?;
    let namespace = reader.read_index(sizes.heap_sizes.string_index)?;
    let extends = TypeDefOrRef::read_from(reader, &sizes.row_counts)?;

    Ok(TypeDefEntry { flags, name, namespace, extends })
  }
}

pub struct TaggedIndex {
  pub tag: u8,
  pub index: u32
}

impl TaggedIndex {
  pub fn tag_mask_from_len(length: u8) -> u8 {
    match length {
      0 => 0b0000_0000,
      1 => 0b0000_0001,
      2 => 0b0000_0011,
      3 => 0b0000_0111,
      4 => 0b0000_1111,
      5 => 0b0001_1111,
      6 => 0b0011_1111,
      7 => 0b0111_1111,
      8 => 0b1111_1111,
      _ => panic!("Invalid tag length: {}", length)
    }
  }

  pub fn read_from<R: Read>(reader: &mut R, tag_bits_length: u8, row_count: u32) -> Result<TaggedIndex> {
    // The maximum number of rows that can be encoded with a 16-bit tagged index
    let max_length_for_word_index = 2u32.pow(16 - (tag_bits_length as u32)) - 1;
    let tag_mask = TaggedIndex::tag_mask_from_len(tag_bits_length);

    let tagged_index = if row_count > max_length_for_word_index {
      reader.read_u32::<LittleEndian>()
    } else {
      reader.read_u16::<LittleEndian>().map(|x| x as u32)
    }?;

    let tag = (tagged_index & (tag_mask as u32)) as u8;
    let index = (tagged_index & !(tag_mask as u32)) >> tag_bits_length;

    Ok (TaggedIndex { tag, index } )
  }
}

macro_rules! max_table_entries {
  ($row_counts: expr, [$x: ident]) => {
    $row_counts.get(&TableId::$x).map(|x|*x).unwrap_or(0u32)
  };
  ($row_counts: expr, [$x: ident, $($tail: ident),+]) => {
    {
      use std::cmp::max;
      max(max_table_entries!($row_counts, [$x]), max_table_entries!($row_counts, [$($tail),+]))
    }
  };
}

macro_rules! tagged_index_parser {
  {
    type: $type_: ident,
    tag_length: $tag_length: expr,
    patterns: [$($pattern: expr => $case: ident),*]
  } => {
    tagged_index_parser! {
      type: $type_,
      tag_length: $tag_length,
      tables: [$($case),*],
      patterns: [$($pattern => $case),+]
    }
  };
  {
    type: $type_: ident,
    tag_length: $tag_length: expr,
    tables: [$($table_id: ident),*],
    patterns: [$($pattern: expr => $case: ident),*]
  } => {
    impl $type_ {
      pub fn read_from<R: Read>(reader: &mut R, row_counts: &RowCounts) -> Result<$type_> {
        let max_size = max_table_entries!(row_counts, [$($table_id),*]);
        let tagged_index = TaggedIndex::read_from(reader, 2, max_size)?;
        match tagged_index.tag {
          $(
            $pattern => Ok($type_::$case(Index::new(tagged_index.index)))
          ),+,
          otherwise => panic!("Invalid tag: {}", otherwise)
        }
      }
    }
  };
}

#[derive(Debug)]
pub enum TypeDefOrRef {
  TypeDef(Index<TypeDefEntry>),
  TypeRef(Index<TypeRefEntry>),
  TypeSpec(Index<TypeSpecEntry>)
}

tagged_index_parser! {
  type: TypeDefOrRef,
  tag_length: 2,
  patterns: [
    0b00 => TypeDef,
    0b01 => TypeRef,
    0b10 => TypeSpec
  ]
}

#[derive(Debug)]
pub enum HasConstant {
  Field(Index<FieldEntry>),
  Param(Index<ParamEntry>),
  Property(Index<PropertyEntry>)
}

tagged_index_parser! {
  type: HasConstant,
  tag_length: 2,
  patterns: [
    0b00 => Field,
    0b01 => Param,
    0b10 => Property
  ]
}

pub enum HasCustomAttribute {
  MethodDef(Index<MethodDefEntry>),
  Field(Index<FieldEntry>),
  TypeRef(Index<TypeRefEntry>),
  TypeDef(Index<TypeDefEntry>),
  Param(Index<ParamEntry>),
  InterfaceImpl(Index<InterfaceImplEntry>),
  MemberRef(Index<MemberRefEntry>),
  Module(Index<ModuleEntry>),
  //Permission(Index<PermissionEntry>),
  Property(Index<PropertyEntry>),
  Event(Index<EventEntry>),
  StandAloneSig(Index<StandAloneSigEntry>),
  ModuleRef(Index<ModuleRefEntry>),
  TypeSpec(Index<TypeSpecEntry>),
  Assembly(Index<AssemblyEntry>),
  AssemblyRef(Index<AssemblyRefEntry>),
  File(Index<FileEntry>),
  ExportedType(Index<ExportedTypeEntry>),
  ManifestResource(Index<ManifestResourceEntry>)
}

tagged_index_parser! {
  type: HasCustomAttribute,
  tag_length: 5,
  patterns: [
    0b00000 => MethodDef,
    0b00001 => Field,
    0b00010 => TypeRef,
    0b00011 => TypeDef,
    0b00100 => Param,
    0b00101 => InterfaceImpl,
    0b00110 => MemberRef,
    0b00111 => Module,
    //0b01000 => Permission,
    0b01001 => Property,
    0b01010 => Event,
    0b01011 => StandAloneSig,
    0b01100 => ModuleRef,
    0b01101 => TypeSpec,
    0b01110 => Assembly,
    0b01111 => AssemblyRef,
    0b10000 => File,
    0b10001 => ExportedType,
    0b10010 => ManifestResource
  ]
}

pub enum HasFieldMarshall {
  Field(Index<FieldEntry>),
  Param(Index<ParamEntry>)
}

tagged_index_parser! {
  type: HasFieldMarshall,
  tag_length: 1,
  patterns: [
    0b0 => Field,
    0b1 => Param
  ]
}

pub enum HasDeclSecurity {
  TypeDef(Index<TypeDefEntry>),
  MethodDef(Index<MethodDefEntry>),
  Assembly(Index<AssemblyEntry>)
}

tagged_index_parser! {
  type: HasDeclSecurity,
  tag_length: 2,
  patterns: [
    0b00 => TypeDef,
    0b01 => MethodDef,
    0b10 => Assembly
  ]
}

pub enum MemberRefParent {
  TypeDef(Index<TypeDefEntry>),
  TypeRef(Index<TypeRefEntry>),
  ModuleRef(Index<ModuleRefEntry>),
  MethodDef(Index<MethodDefEntry>),
  TypeSpec(Index<TypeSpecEntry>)
}

tagged_index_parser! {
  type: MemberRefParent,
  tag_length: 3,
  patterns: [
    0b000 => TypeDef,
    0b001 => TypeRef,
    0b010 => ModuleRef,
    0b011 => MethodDef,
    0b100 => TypeSpec
  ]
}

pub enum HasSemantics {
  Event(Index<EventEntry>),
  Property(Index<PropertyEntry>)
}

tagged_index_parser! {
  type: HasSemantics,
  tag_length: 1,
  patterns: [
    0b0 => Event,
    0b1 => Property
  ]
}

pub enum MethodDefOrRef {
  MethodDef(Index<MethodDefEntry>),
  MethodRef(Index<MemberRefEntry>)
}

tagged_index_parser! {
  type: MethodDefOrRef,
  tag_length: 1,
  tables: [MethodDef, MemberRef],
  patterns: [
    0b0 => MethodDef,
    0b1 => MethodRef
  ]
}

pub enum MemberForwarded {
  Field(Index<FieldEntry>),
  MethodDef(Index<MemberRefEntry>)
}

tagged_index_parser! {
  type: MemberForwarded,
  tag_length: 1,
  patterns: [
    0b0 => Field,
    0b1 => MethodDef
  ]
}

pub enum Implementation {
  File(Index<FileEntry>),
  AssemblyRef(Index<AssemblyRefEntry>),
  ExportedType(Index<ExportedTypeEntry>)
}

tagged_index_parser! {
  type: Implementation,
  tag_length: 2,
  patterns: [
    0b00 => File,
    0b01 => AssemblyRef,
    0b10 => ExportedType
  ]
}

pub enum CustomAttributeType {
  MethodDef(Index<MethodDefEntry>),
  MemberRef(Index<MemberRefEntry>)
}

tagged_index_parser! {
  type: CustomAttributeType,
  tag_length: 3,
  patterns: [
    0b010 => MethodDef,
    0b011 => MemberRef
  ]
}

#[derive(Debug)]
pub enum ResolutionScope {
  Module(Index<ModuleEntry>),
  ModuleRef(Index<TypeDefEntry>),
  AssemblyRef(Index<TypeDefEntry>),
  TypeRef(Index<TypeDefEntry>)
}

tagged_index_parser! {
  type: ResolutionScope,
  tag_length: 2,
  patterns: [
      0b00 => Module,
      0b01 => ModuleRef,
      0b10 => AssemblyRef,
      0b11 => TypeRef
  ]
}

pub enum TypeOrMethodDef {
  TypeDef(Index<TypeDefEntry>),
  MethodDef(Index<MethodDefEntry>)
}

tagged_index_parser! {
  type: TypeOrMethodDef,
  tag_length: 1,
  patterns: [
    0b0 => TypeDef,
    0b1 => MethodDef
  ]
}

#[derive(Debug)]
pub struct StringHeap;
#[derive(Debug)]
pub struct GuidHeap;
#[derive(Debug)]
pub struct BlobHeap;

bitflags! {
  // https://github.com/dotnet/coreclr/blob/master/src/inc/corhdr.h#L276
  pub flags TypeAttributes: u32 {
    // Use this mask to retrieve the type visibility information.
    const tdVisibilityMask        =   0x00000007,
    const tdNotPublic             =   0x00000000,     // Class is not public scope.
    const tdPublic                =   0x00000001,     // Class is public scope.
    const tdNestedPublic          =   0x00000002,     // Class is nested with public visibility.
    const tdNestedPrivate         =   0x00000003,     // Class is nested with private visibility.
    const tdNestedFamily          =   0x00000004,     // Class is nested with family visibility.
    const tdNestedAssembly        =   0x00000005,     // Class is nested with assembly visibility.
    const tdNestedFamANDAssem     =   0x00000006,     // Class is nested with family and assembly visibility.
    const tdNestedFamORAssem      =   0x00000007,     // Class is nested with family or assembly visibility.

    // Use this mask to retrieve class layout information
    const tdLayoutMask            =   0x00000018,
    const tdAutoLayout            =   0x00000000,     // Class fields are auto-laid out
    const tdSequentialLayout      =   0x00000008,     // Class fields are laid out sequentially
    const tdExplicitLayout        =   0x00000010,     // Layout is supplied explicitly
    // end layout mask

    // Use this mask to retrieve class semantics information.
    const tdClassSemanticsMask    =   0x00000060,
    const tdClass                 =   0x00000000,     // Type is a class.
    const tdInterface             =   0x00000020,     // Type is an interface.
    // end semantics mask

    // Special semantics in addition to class semantics.
    const tdAbstract              =   0x00000080,     // Class is abstract
    const tdSealed                =   0x00000100,     // Class is concrete and may not be extended
    const tdSpecialName           =   0x00000400,     // Class name is special. Name describes how.

    // Implementation attributes.
    const tdImport                =   0x00001000,     // Class / interface is imported
    const tdSerializable          =   0x00002000,     // The class is Serializable.

    // Use tdStringFormatMask to retrieve string information for native interop
    const tdStringFormatMask      =   0x00030000,
    const tdAnsiClass             =   0x00000000,     // LPTSTR is interpreted as ANSI in this class
    const tdUnicodeClass          =   0x00010000,     // LPTSTR is interpreted as UNICODE
    const tdAutoClass             =   0x00020000,     // LPTSTR is interpreted automatically
    const tdCustomFormatClass     =   0x00030000,     // A non-standard encoding specified by CustomFormatMask
    const tdCustomFormatMask      =   0x00C00000,     // Use this mask to retrieve non-standard encoding information for native interop. The meaning of the values of these 2 bits is unspecified.

    // end string format mask

    const tdBeforeFieldInit       =   0x00100000,     // Initialize the class any time before first static field access.
    const tdForwarder             =   0x00200000,     // This ExportedType is a type forwarder.

    // Flags reserved for runtime use.
    const tdReservedMask          =   0x00040800,
    const tdRTSpecialName         =   0x00000800,     // Runtime should check name encoding.
    const tdHasSecurity           =   0x00040000,     // Class has security associate with it.
  }
}
