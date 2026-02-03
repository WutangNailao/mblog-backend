use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_resource")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub public_id: String,
    pub memo_id: i32,
    pub user_id: i32,
    pub file_type: String,
    pub file_name: String,
    pub file_hash: String,
    pub size: i64,
    pub internal_path: Option<String>,
    pub external_link: Option<String>,
    pub storage_type: Option<String>,
    pub created: Option<DateTimeUtc>,
    pub updated: Option<DateTimeUtc>,
    pub suffix: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
