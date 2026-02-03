package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.KeyType;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;
import java.sql.Timestamp;


@Setter
@Getter
@Table(value = "t_user")
public class TUser implements Serializable {


    @Id(keyType = KeyType.Auto)
    private Integer id;

    
    private String username;

    
    private String passwordHash;

    
    private String email;

    
    private String displayName;

    
    private String bio;

    
    private Timestamp created;

    
    private Timestamp updated;

    
    private String role;

    
    private String avatarUrl;

    
    private Timestamp lastClickedMentioned;

    
    private String defaultVisibility;
    private String defaultEnableComment;

}
