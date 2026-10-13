
use super::{is_xsd, try_sample, try_schema};

#[test]
fn person_becomes_an_xsd() {
    let src = r#"<person id="1"><name>Ada</name><age>36</age></person>"#;
    assert_eq!(
        try_schema(src).as_deref(),
        Some(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="person">
        <xs:complexType>
            <xs:sequence>
                <xs:element name="name" type="xs:string"/>
                <xs:element name="age" type="xs:int"/>
            </xs:sequence>
            <xs:attribute name="id" type="xs:int" use="required"/>
        </xs:complexType>
    </xs:element>
</xs:schema>"#
        )
    );
}

#[test]
fn repeated_children_are_optional_when_missing() {
    let src = "\
<list>
  <item><id>1</id><name>a</name></item>
  <item><id>2</id></item>
</list>";
    let schema = try_schema(src).unwrap();
    assert!(schema.contains("<xs:element name=\"item\" maxOccurs=\"unbounded\">"));
    assert!(schema.contains("<xs:element name=\"id\" type=\"xs:int\"/>"));
    assert!(schema.contains("<xs:element name=\"name\" type=\"xs:string\" minOccurs=\"0\"/>"));
    assert!(!schema.contains("copycraft"));
}

#[test]
fn scalars_widen_like_numbers() {
    let src = "<list><n>1</n><n>3000000000</n><n>1.5</n><flag>true</flag><flag>false</flag></list>";
    let schema = try_schema(src).unwrap();
    assert!(schema.contains("<xs:element name=\"n\" type=\"xs:double\" maxOccurs=\"unbounded\"/>"));
    assert!(
        schema.contains("<xs:element name=\"flag\" type=\"xs:boolean\" maxOccurs=\"unbounded\"/>")
    );
}

#[test]
fn conflicting_text_becomes_string() {
    let schema = try_schema("<list><a>1</a><a>no</a></list>").unwrap();
    assert!(schema.contains("<xs:element name=\"a\" type=\"xs:string\" maxOccurs=\"unbounded\"/>"));
}

#[test]
fn empty_element_is_an_empty_complex_type() {
    let schema = try_schema("<root><item/></root>").unwrap();
    assert!(schema.contains("<xs:element name=\"item\">"));
    assert!(schema.contains("<xs:complexType/>"));
}

#[test]
fn text_with_an_attribute_uses_simple_content() {
    let schema = try_schema(r#"<title lang="en">Hello</title>"#).unwrap();
    assert!(schema.contains("<xs:simpleContent>"));
    assert!(schema.contains("base=\"xs:string\""));
    assert!(schema.contains("name=\"lang\" type=\"xs:string\" use=\"required\""));
}

#[test]
fn declaration_comment_and_entities_are_read() {
    let src = r#"<?xml version="1.0"?><!-- note --><root><name>A &amp; B</name></root>"#;
    let schema = try_schema(src).unwrap();
    assert!(schema.contains("name=\"root\""));
    assert!(schema.contains("name=\"name\" type=\"xs:string\""));
}

#[test]
fn prefixed_names_use_the_local_name() {
    let schema = try_schema(r#"<h:table xmlns:h="urn:example"><h:tr/></h:table>"#).unwrap();
    assert!(schema.contains("name=\"table\""));
    assert!(schema.contains("name=\"tr\""));
    assert!(!schema.contains("urn:example"));
}

#[test]
fn non_xml_and_broken_markup_have_no_schema() {
    assert!(try_schema("hello").is_none());
    assert!(try_schema("<a></b>").is_none());
    assert!(try_schema("<a></a><b></b>").is_none());
    assert!(try_schema("<root><item></item>").is_none());
}

#[test]
fn person_xsd_sample_round_trips() {
    let src = r#"<person id="1"><name>Ada</name><age>36</age></person>"#;
    let schema = try_schema(src).unwrap();
    assert!(is_xsd(&schema));
    assert_eq!(
        try_sample(&schema).as_deref(),
        Some("<person id=\"1\">\n    <name>sample</name>\n    <age>1</age>\n</person>")
    );
}

#[test]
fn repeated_optional_children_are_filled_twice() {
    let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="list">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="item" maxOccurs="unbounded">
          <xs:complexType>
            <xs:sequence>
              <xs:element name="id" type="xs:int"/>
              <xs:element name="name" type="xs:string" minOccurs="0"/>
            </xs:sequence>
          </xs:complexType>
        </xs:element>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
    assert_eq!(
        try_sample(schema).as_deref(),
        Some(
            "<list>\n    <item>\n        <id>1</id>\n        <name>sample</name>\n    </item>\n    <item>\n        <id>2</id>\n        <name>sample 2</name>\n    </item>\n</list>"
        )
    );
}

#[test]
fn simple_content_includes_the_attribute() {
    let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="title">
    <xs:complexType>
      <xs:simpleContent>
        <xs:extension base="xs:string">
          <xs:attribute name="lang" type="xs:string"/>
          <xs:attribute name="gone" type="xs:string" use="prohibited"/>
        </xs:extension>
      </xs:simpleContent>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
    assert_eq!(
        try_sample(schema).as_deref(),
        Some("<title lang=\"sample\">sample</title>")
    );
}

#[test]
fn named_complex_type_is_preferred_over_a_builtin() {
    let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="person" type="Person"/>
  <xs:complexType name="Person">
    <xs:sequence>
      <xs:element name="name" type="xs:string"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:int" use="required"/>
  </xs:complexType>
  <xs:element name="box" type="string"/>
  <xs:complexType name="string">
    <xs:sequence>
      <xs:element name="value" type="xs:int"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#;
    assert_eq!(
        try_sample(schema).as_deref(),
        Some("<person id=\"1\">\n    <name>sample</name>\n</person>")
    );
    let box_schema = schema.replace(r#"<xs:element name="person" type="Person"/>"#, "");
    assert_eq!(
        try_sample(&box_schema).as_deref(),
        Some("<box>\n    <value>1</value>\n</box>")
    );
}

#[test]
fn choice_ref_extension_and_recursion_stay_bounded() {
    let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="employee" type="Employee"/>
  <xs:element name="name" type="xs:string"/>
  <xs:complexType name="Named">
    <xs:sequence>
      <xs:element ref="name"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:int"/>
  </xs:complexType>
  <xs:complexType name="Employee">
    <xs:complexContent>
      <xs:extension base="Named">
        <xs:sequence>
          <xs:choice>
            <xs:element name="role" type="xs:string"/>
            <xs:element name="title" type="xs:string"/>
          </xs:choice>
        </xs:sequence>
        <xs:attribute name="dept" type="xs:string"/>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
</xs:schema>"#;
    assert_eq!(
        try_sample(schema).as_deref(),
        Some(
            "<employee id=\"1\" dept=\"sample\">\n    <name>sample</name>\n    <role>sample</role>\n</employee>"
        )
    );
    let recursive = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="node">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="node" minOccurs="0" type="xs:string"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
    assert_eq!(
        try_sample(recursive).as_deref(),
        Some("<node>\n    <node/>\n</node>")
    );
}

#[test]
fn non_xsd_and_empty_schema_have_no_sample() {
    assert!(!is_xsd("hello"));
    assert!(try_sample("hello").is_none());
    assert!(!is_xsd(r#"<person id="1"><name>Ada</name></person>"#));
    assert!(try_sample(r#"<person id="1"><name>Ada</name></person>"#).is_none());
    let empty = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"/>"#;
    assert!(is_xsd(empty));
    assert!(try_sample(empty).is_none());
}
