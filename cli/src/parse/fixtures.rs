//! Sample documents shared by more than one test module in `parse`.
//!
//! `SAMPLE_FULL` is a real AOxml header, trimmed: the parser tests read the
//! fields out of it and [`super::classify`] checks that it is recognised as a
//! manuscript at all. Keeping one copy means the two cannot drift into testing
//! different documents while appearing to test the same one.

/// A complete AOxml header with the roles the editor rules rank against.
pub(super) const SAMPLE_FULL: &str = r#"<?xml-stylesheet href="HPMxml.css" type="text/css"?>
<AOxml xmlns:AO="http://hethiter.net/ns/AO/1.0">
<AOHeader>
  <docID>KBo 17.86+</docID>
  <meta>
    <creation-date date="2016-04-15T16:55:36.58"/>
    <kor2 date="2021-04-22T09:07:54"/>
    <annotation>
      <annot editor="auto" date=""/>
      <annot editor="" date=""/>
    </annotation>
    <neu>
      <uebern editor="FB" date="2017-03-28" src="MZ"/>
      <kor1kf editor="FB" date="2017-06-02"/>
      <kor editor="SG" date="2020-05-27"/>
      <annot editor="UG" date="2021-04-26"/>
    </neu>
  </meta>
</AOHeader>
<body>
  <AO:Manuscripts><AO:TxtPubl>KBo 17.86 {€1}+KBo 15.62 {€2}</AO:TxtPubl></AO:Manuscripts>
</body>
</AOxml>"#;
