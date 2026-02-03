package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;


@Setter
@Getter
@Table(value = "t_user_config")
public class TUserConfig implements Serializable {

    
    @Id
    private Integer userId;

    
    @Id
    private String key;

    
    private String value;

    
    private String defaultValue;

}
