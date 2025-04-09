use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "person_auth_google")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, default = "uuid_generate_v4()")]
    pub id: uuid::Uuid,

    #[sea_orm(column_type = "Uuid")]
    pub person_id: uuid::Uuid,

    #[sea_orm(column_type = "Text")]
    pub auth_provider_user_id: String,

    #[sea_orm(column_type = "Text")]
    pub auth_provider_access_token: String,

    #[sea_orm(column_type = "Text")]
    pub auth_provider_refresh_token: String,

    #[sea_orm(column_type = "Text")]
    pub email: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::person::Entity",
        from = "Column::PersonId",
        to = "super::person::Column::Id"
    )]
    Person,
}

impl Related<super::person::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Person.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
