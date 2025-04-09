use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "waiting_room")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, default = "uuid_generate_v4()")]
    pub id: uuid::Uuid,

    /// token supplied by device
    #[sea_orm(column_type = "Text", unique, indexed)]
    pub device_temporary_token: String,

    pub first_seen: DateTime,

    pub expires_at: DateTime,

    #[sea_orm(column_type = "Text", unique, indexed)]
    pub registration_token: String,

    #[sea_orm(column_type = "Text", unique, indexed)]
    pub registration_url: String,

    #[sea_orm(column_type = "Uuid", indexed, nullable)]
    pub resulting_device_id: Option<uuid::Uuid>,

    #[sea_orm(column_type = "Text")]
    pub api_token: Option<String>,

    #[sea_orm(column_type = "Text")]
    pub api_endpoint: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::device::Entity",
        from = "Column::ResultingDeviceId",
        to = "super::device::Column::Id"
    )]
    Device,
}

impl Related<super::device::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
